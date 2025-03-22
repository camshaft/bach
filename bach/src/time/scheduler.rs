use super::{
    entry::atomic::{self, ArcEntry},
    wheel::Wheel,
};
use crate::{queue, sync::queue::Shared as _};
use alloc::sync::Arc;
use core::{
    fmt,
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicU64, Ordering},
    task::{Context, Poll},
};
use std::sync::Mutex;

crate::scope::define!(scope, Handle);

pub(crate) fn ticks() -> u64 {
    scope::borrow_with(|h| h.ticks())
}

type Queue = Arc<Mutex<queue::span::Queue<queue::vec_deque::Queue<ArcEntry>>>>;

fn new_queue() -> Queue {
    let queue = queue::vec_deque::Queue::default();
    let queue = queue::span::Queue::new(queue, "bach::timer");
    let queue = Mutex::new(queue);
    Arc::new(queue)
}

pub struct Scheduler {
    wheel: Wheel<ArcEntry>,
    handle: Handle,
    queue: Queue,
}

impl fmt::Debug for Scheduler {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Scheduler")
            .field("ticks", &self.handle.ticks())
            .field("wheel", &self.wheel)
            .finish()
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    /// Creates a new Scheduler
    pub fn new() -> Self {
        let queue = new_queue();

        let handle = Handle::new(queue.clone());

        Self {
            wheel: Default::default(),
            handle,
            queue,
        }
    }

    /// Returns a handle that can be easily cloned
    pub fn handle(&self) -> Handle {
        self.handle.clone()
    }

    pub fn enter<F: FnOnce() -> O, O>(&self, f: F) -> O {
        let (_, res) = scope::with(self.handle(), f);
        res
    }

    /// Returns the amount of time until the next task
    ///
    /// An implementation may sleep for the duration.
    pub fn advance(&mut self) -> Option<u64> {
        self.collect();

        let ticks = self.wheel.advance()?;
        self.handle.advance(ticks);

        Some(ticks)
    }

    /// Wakes all of the expired tasks
    pub fn wake(&mut self) -> usize {
        let (_, res) = scope::with(self.handle(), || self.wheel.wake(atomic::wake));
        res
    }

    /// Move the queued entries into the wheel
    pub fn collect(&mut self) {
        let _ = scope::with(self.handle(), || {
            for entry in self.queue.lock().unwrap().drain() {
                self.wheel.insert(entry);
            }
        });
    }

    pub fn close(&mut self) {
        let _ = scope::with(self.handle(), || {
            self.wheel.close(|entry| {
                // notify everything that we're shutting down
                entry.wake();
            })
        });
    }

    pub fn reset(&mut self) {
        self.wheel.reset();
    }
}

#[derive(Debug, Clone)]
pub struct Handle(Arc<InnerHandle>);

impl Handle {
    fn new(queue: Queue) -> Self {
        let inner = InnerHandle {
            ticks: AtomicU64::new(0),
            queue,
        };
        Self(Arc::new(inner))
    }

    /// Returns a future that sleeps for the given number of ticks
    pub fn delay(&self, ticks: u64) -> Timer {
        let entry = atomic::Entry::new(ticks);
        let handle = self.clone();
        Timer { handle, entry }
    }

    /// Returns the number of ticks that has passed for this scheduler
    pub fn ticks(&self) -> u64 {
        self.0.ticks.load(Ordering::SeqCst)
    }

    /// Returns the current time for the scheduler
    pub fn now(&self) -> super::Instant {
        let ticks = self.ticks();
        let duration = crate::time::resolution::ticks_to_duration(ticks);
        super::Instant(duration)
    }

    fn advance(&self, ticks: u64) {
        if cfg!(test) {
            self.0
                .ticks
                .load(Ordering::SeqCst)
                .checked_add(ticks)
                .expect("tick overflow");
        }
        self.0.ticks.fetch_add(ticks, Ordering::SeqCst);
    }
}

#[derive(Debug)]
struct InnerHandle {
    ticks: AtomicU64,
    queue: Queue,
}

impl Handle {
    fn register(&self, entry: &ArcEntry) {
        let _ = self.0.queue.push_lazy(&mut Some(entry.clone()));
    }
}

/// A future that sleeps a task for a duration
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Timer {
    handle: Handle,
    entry: ArcEntry,
}

impl Timer {
    pub fn reset(&mut self, target: super::Instant) {
        *self = super::sleep_until(target);
    }

    /// Cancels the timer
    pub fn cancel(&mut self) {
        self.entry.cancel();
    }
}

impl fmt::Debug for Timer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Timer").finish()
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl Future for Timer {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<()> {
        // check condition before to avoid needless registration
        if self.entry.take_expired() {
            return Poll::Ready(());
        }

        // register the waker with the entry
        self.entry.register(cx.waker());

        // check condition after registration to avoid loss of notification
        if self.entry.take_expired() {
            return Poll::Ready(());
        }

        // register the timer with the handle
        if self.entry.should_register() {
            self.handle.register(&self.entry);
        }

        Poll::Pending
    }
}
