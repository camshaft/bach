extern crate alloc;

#[macro_use]
mod tracing;
#[macro_use]
pub mod metrics;

pub mod coop;
pub mod environment;
pub mod executor;
pub mod ext;
pub mod group;
#[cfg(any(test, feature = "net"))]
pub mod net;
pub mod rand;
pub mod scope;
pub mod sync;
pub mod task;
pub mod time;

/// Returns `true` if the caller is being executed in a `bach` environment
pub fn is_active() -> bool {
    task::scope::try_borrow_with(|scope| scope.is_some())
}
