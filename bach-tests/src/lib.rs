use bach::environment::default::Runtime;

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
);

pub mod benches;
pub mod testing;

#[cfg(not(feature = "leaks"))]
pub fn sim(f: impl FnOnce()) -> std::time::Duration {
    crate::testing::init_tracing();
    let mut rt = Runtime::new();
    rt.run(f);
    rt.elapsed()
}

#[cfg(feature = "leaks")]
pub fn sim(f: impl FnOnce()) -> std::time::Duration {
    use checkers::Violation;

    let mut time = std::time::Duration::ZERO;
    let snapshot = checkers::with(|| {
        let mut rt = Runtime::default();
        // initialize all of the thread locals
        rt.run(f);
        time = rt.elapsed();
        drop(rt);
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

    time
}
