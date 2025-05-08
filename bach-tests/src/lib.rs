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

    violations.retain(|v| {
        match v {
            Violation::Leaked { alloc } => {
                if let Some(bt) = &alloc.backtrace {
                    for frame in bt.frames() {
                        for sym in frame.symbols() {
                            let sym = format!("{sym:?}");

                            // bolero never cleans up the default state to avoid branching
                            if sym.contains("bolero_generator::any::default::with") {
                                return false;
                            }
                        }
                    }
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
