#![doc = include_str!("../README.md")]

extern crate alloc;

#[macro_use]
mod tracing;
#[macro_use]
pub mod metrics;

pub mod coop;
pub mod cost;
pub mod environment;
pub mod executor;
pub mod ext;
pub mod group;
#[cfg(feature = "net")]
pub mod net;
pub mod queue;
pub mod rand;
pub mod runtime;
pub mod scope;
pub mod sync;
pub mod task;
pub mod time;

pub use task::spawn;

/// Returns `true` if the caller is being executed in a `bach` environment
pub fn is_active() -> bool {
    task::scope::try_borrow_with(|scope| scope.is_some())
}

/// Runs a simulation using the default environment
pub fn sim<F: FnOnce() -> R, R>(f: F) -> R {
    let mut rt = environment::default::Runtime::new();
    rt.run(f)
}
