extern crate alloc;

mod queue;

pub mod environment;
pub mod executor;
pub mod ext;
pub mod group;
pub mod rand;
pub mod scope;
pub mod task;
pub mod time;

/// Returns `true` if the caller is being executed in a `bach` environment
pub fn is_active() -> bool {
    task::scope::try_borrow_with(|scope| scope.is_some())
}
