use crate::sim;
use bach::ext::*;

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
    });
}
