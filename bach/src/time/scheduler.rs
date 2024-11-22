use super::{
    entry::atomic::{self, ArcEntry},
    wheel::Wheel,
};
use crate::sync::queue::{self, Queue as _};
use alloc::sync::Arc;
use core::{
    fmt,
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicU64, Ordering},
    task::{Context, Poll},
};

crate::scope::define!(scope, Handle);

type Queue = Arc<queue::span::Queue<queue::vec_deque::Queue<ArcEntry>>>;

fn new_queue() -> Queue {
    let queue = queue::vec_deque::Queue::default();
    let queue = queue::span::Queue::new(queue, "bach::timer");
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
        scope::with(self.handle(), f)
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
        scope::with(self.handle(), || self.wheel.wake(atomic::wake))
    }

    /// Move the queued entries into the wheel
    pub fn collect(&mut self) {
        scope::with(self.handle(), || {
            for entry in self.queue.drain() {
                self.wheel.insert(entry);
            }
        })
    }

    pub fn close(&mut self) {
        scope::with(self.handle(), || {
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
        let _ = self.0.queue.push(entry.clone());
    }
}

/// A future that sleeps a task for a duration
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Timer {
    handle: Handle,
    entry: ArcEntry,
}

impl Timer {
    /// Cancels the timer
    pub fn cancel(&mut self) {
        self.entry.cancel();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::tests::executor;
    use alloc::vec::Vec;
    use bolero::{check, generator::*};
    use core::time::Duration;

    async fn delay(handle: Handle, count: usize, delay: u64) {
        for _ in 0..count {
            // get the time before the delay
            let now = handle.ticks();

            // await the delay
            handle.delay(delay).await;

            // get the time that has passed on the clock and make sure it matches the amount that
            // was delayed
            let actual = handle.ticks();
            let expected = now + delay;
            assert_eq!(
                actual, expected,
                "actual: {:?}, expected: {:?}",
                actual, expected
            );
        }
    }

    fn test_helper(delays: &[(u8, u64)]) {
        let mut scheduler = Scheduler::new();
        let mut executor = executor();

        let handle = scheduler.handle();

        for (count, duration) in delays.as_ref().iter() {
            executor.spawn(delay(handle.clone(), *count as usize, *duration));
        }

        let mut total = 0;

        loop {
            executor.macrostep();

            if let Some(expiration) = scheduler.advance() {
                total += expiration;
                scheduler.wake();
            } else if scheduler.wake() == 0 {
                // there are no remaining tasks to execute
                break;
            }
        }

        assert!(
            executor.microstep().is_ready(),
            "the task list should be empty"
        );
        assert!(
            scheduler.advance().is_none(),
            "the scheduler should be empty"
        );
        assert_eq!(
            total,
            handle.ticks(),
            "the current time should reflect the total number of sleep durations"
        );

        drop(scheduler);
        drop(executor);

        assert_eq!(
            Arc::strong_count(&handle.0),
            1,
            "the scheduler should cleanly shut down"
        );
    }

    #[test]
    fn timer_test() {
        crate::testing::init_tracing();

        let min_time = Duration::from_nanos(1).as_nanos() as u64;
        let max_time = Duration::from_secs(3600).as_nanos() as u64;

        let delay = min_time..max_time;
        let count = 0u8..3;
        let delays = gen::<Vec<_>>().with().values((count, delay));

        check!()
            .with_generator(delays)
            .for_each(|delays| test_helper(delays));
    }
}
