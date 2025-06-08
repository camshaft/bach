use bach::{
    ext::*,
    sync::{Mutex, Semaphore},
};
use std::sync::Arc;

mod mpsc;
mod mutex;
mod rwlock;
mod semaphore;

#[cfg(test)]
pub fn sim(f: impl Fn()) -> impl Fn() {
    crate::testing::init_tracing();
    move || {
        let mut rt = bach::environment::default::Runtime::new()
            .with_coop(true)
            .with_rand(None);
        rt.run(&f);
    }
}
