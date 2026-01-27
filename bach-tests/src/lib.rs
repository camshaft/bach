#[cfg(feature = "leaks")]
#[global_allocator]
static ALLOC: checkers::Allocator = checkers::Allocator::system();
use std::backtrace::BacktraceStatus;


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
            checkers::Violation::Leaked { alloc } => {
                if bt_matches(alloc, |bt| {
                    bt.contains("bolero_generator::any::default::with")
                        || bt.contains("bach::group::Groups::name_to_id")
                }) {
                    return false;
                }
            }
            checkers::Violation::MissingFree { request } => {
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
