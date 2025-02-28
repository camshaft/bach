use bach::{environment::default::Runtime, ext::*};

fn sim(f: impl Fn()) -> impl Fn() {
    crate::testing::init_tracing();
    move || {
        let mut rt = Runtime::new().with_rand(Some(bach::rand::Scope::new(123)));
        rt.run(&f);
    }
}

/// Ensures that a task that panics doesn't cause the simulation to double panic
#[test]
#[should_panic = "panic"]
fn task_panic() {
    sim(|| {
        async move {
            panic!("panic");
        }
        .primary()
        .spawn();
    })();
}
