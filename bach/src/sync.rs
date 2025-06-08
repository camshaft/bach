pub mod channel;
pub mod duplex;
pub mod mpsc;
mod mutex;
pub mod queue;
mod rwlock;
mod semaphore;

pub use mutex::{Mutex, MutexGuard};
pub use rwlock::{RwLock, RwLockReadGuard, RwLockWriteGuard};
pub use semaphore::{Semaphore, SemaphorePermit};
// One shot is spsc so it doesn't need a coop wrapper
pub use tokio::sync::oneshot;
