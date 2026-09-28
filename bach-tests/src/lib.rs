#[cfg(feature = "leaks")]
#[global_allocator]
static ALLOC: checkers::Allocator = checkers::Allocator::system();

macro_rules! tests {
    ($($(#[cfg($($tt:tt)*)])? $name:ident),* $(,)?) => {
        $(
            $(#[cfg($($tt)*)])?
            #[cfg(test)]
            mod $name;
        )*
    };
}

tests!(
    #[cfg(feature = "coop")]
    coop,
    #[cfg(feature = "net")]
    net,
    panics,
    queue,
    #[cfg(feature = "coop")]
    sync,
    task,
    time,
);

pub mod benches;
pub mod testing;

#[cfg(not(feature = "leaks"))]
pub fn sim<F: FnOnce()>(f: F) {
    crate::testing::init_tracing();
    bach::sim(f);
}

#[cfg(feature = "leaks")]
pub fn sim<F: FnOnce()>(f: F) {
    use checkers::Violation;
    use std::backtrace::BacktraceStatus;

    let snapshot = checkers::with(|| {
        bach::sim(f);
    });

    let mut violations = vec![];
    snapshot.validate(&mut violations);

    fn bt_matches(req: &checkers::Request, predicate: impl Fn(&str) -> bool) -> bool {
        // If backtraces aren’t enabled/captured, there’s nothing meaningful to filter on.
        if req.backtrace.status() != BacktraceStatus::Captured {
            return false;
        }

        // std::backtrace doesn't give stable frame iteration; use its Debug output.
        let bt = format!("{:?}", req.backtrace);
        predicate(&bt)
    }

    violations.retain(|v| {
        match v {
            Violation::Leaked { alloc } => {
                if bt_matches(alloc, |bt| {
                    bt.contains("bolero_generator::any::default::with")
                        // The group name/id intern maps live in a thread-local `Groups` for the
                        // life of the thread (freed at thread exit), so their interned name
                        // Strings are still allocated at end-of-sim — an intentional cache, not a
                        // leak. Match on the type path only (not `...::name_to_id`): rustc renders
                        // the frame as `<bach::group::Groups>::name_to_id` on newer toolchains, so
                        // the angle brackets break a `Groups::name_to_id` substring — `bach::group::Groups`
                        // matches both the old `Type::method` and new `<Type>::method` renderings.
                        || bt.contains("bach::group::Groups")
                }) {
                    return false;
                }
            }
            Violation::MissingFree { request } => {
                if bt_matches(request, |bt| bt.contains("thread_local::destructors")) {
                    return false;
                }
            }
            _ => {
                // TODO
            }
        }

        true
    });

    assert!(violations.is_empty(), "{violations:?}");
}
