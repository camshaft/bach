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

    let snapshot = checkers::with(|| {
        bach::sim(f);
    });

    let mut violations = vec![];
    snapshot.validate(&mut violations);

    fn bt_matches(req: &checkers::Request, predicate: impl Fn(&str) -> bool) -> bool {
        if let Some(bt) = &req.backtrace {
            for frame in bt.frames() {
                for sym in frame.symbols() {
                    let sym = format!("{sym:?}");

                    if predicate(&sym) {
                        return true;
                    }
                }
            }
        }

        false
    }

    violations.retain(|v| {
        match v {
            Violation::Leaked { alloc } => {
                if bt_matches(alloc, |sym| {
                    [
                        "bolero_generator::any::default::with",
                        "bach::group::Groups::name_to_id",
                    ]
                    .iter()
                    .any(|pat| sym.contains(pat))
                }) {
                    return false;
                }
            }
            Violation::MissingFree { request } => {
                if bt_matches(request, |sym| {
                    ["thread_local::destructors"]
                        .iter()
                        .any(|pat| sym.contains(pat))
                }) {
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
