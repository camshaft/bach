use crate::coop::Operation;
use std::{
    cell::RefCell,
    sync::{Arc, Condvar, Mutex},
    task::{Context, Poll},
};

thread_local! {
    /// The fiber context for the current thread, if this thread is a fiber.
    pub(crate) static CURRENT_FIBER: RefCell<Option<Arc<dyn FiberContext>>> = RefCell::new(None);
}

/// Interface exposed by a running fiber to synchronous code (e.g. `blocking_lock`).
pub(crate) trait FiberContext: Send + Sync {
    /// Park this fiber, registering `operation` with the coop scheduler.
    ///
    /// Returns once the executor has granted the operation and resumed the fiber.
    fn park_for_operation(&self, operation: Operation);
}

/// Tracks the lifecycle state shared between the executor future and the fiber thread.
#[derive(Clone, Copy)]
enum FiberStatus {
    /// Thread should be (or will be) given the run signal.
    WillRun,
    /// Thread is currently executing.
    Running,
    /// Thread has parked at a blocking call, waiting for `operation`.
    Parked(Operation),
    /// Thread has finished (result is in `FiberShared::result`).
    Done,
}

struct FiberShared<R: Send + 'static> {
    status: Mutex<FiberStatus>,
    result: Mutex<Option<std::thread::Result<R>>>,
    /// Signals the fiber thread to run or resume.
    fiber_wakeup: Condvar,
    /// Signals the executor that the fiber has parked or finished.
    executor_wakeup: Condvar,
}

impl<R: Send + 'static> FiberContext for FiberShared<R> {
    fn park_for_operation(&self, operation: Operation) {
        let mut status = self.status.lock().unwrap();
        *status = FiberStatus::Parked(operation);
        // Tell the executor we are parked.
        self.executor_wakeup.notify_one();
        // Sleep until the executor transitions us back to Running.
        let _guard = self
            .fiber_wakeup
            .wait_while(status, |s| !matches!(s, FiberStatus::Running))
            .unwrap();
    }
}

fn run_fiber<R: Send + 'static>(
    shared: Arc<FiberShared<R>>,
    f: Box<dyn FnOnce() -> R + Send + 'static>,
) {
    // Block until the executor gives us the initial run signal.
    {
        let status = shared.status.lock().unwrap();
        let _guard = shared
            .fiber_wakeup
            .wait_while(status, |s| !matches!(s, FiberStatus::Running))
            .unwrap();
    }

    // Register this thread as the current fiber so that `blocking_lock` can
    // find the park channel.
    let context: Arc<dyn FiberContext> = shared.clone();
    CURRENT_FIBER.with(|c| *c.borrow_mut() = Some(context));

    // Run the user closure, catching any panics so we can forward them.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));

    // Deregister before storing the result so that any Drop impls called
    // during result storage don't see a stale fiber context.
    CURRENT_FIBER.with(|c| *c.borrow_mut() = None);

    // Store the result and signal the executor.
    *shared.result.lock().unwrap() = Some(result);
    *shared.status.lock().unwrap() = FiberStatus::Done;
    shared.executor_wakeup.notify_one();
}

/// A [`Future`] that drives a synchronous closure on a dedicated OS thread.
///
/// While the closure runs, calls to [`Mutex::blocking_lock`](crate::sync::Mutex::blocking_lock)
/// that encounter contention park the OS thread at the call site.  The Bach
/// executor registers the contended coop [`Operation`] and resumes the thread
/// once the scheduler grants it — so the closure genuinely resumes at the exact
/// line of the `blocking_lock` call rather than restarting from `poll`.
///
/// Since all fields are [`Unpin`], `FiberFuture<R>` is `Unpin` and can be
/// moved freely before the first `poll`.
pub struct FiberFuture<R: Send + 'static> {
    f: Option<Box<dyn FnOnce() -> R + Send + 'static>>,
    shared: Arc<FiberShared<R>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl<R: Send + 'static> FiberFuture<R> {
    pub fn new<F: FnOnce() -> R + Send + 'static>(f: F) -> Self {
        Self {
            f: Some(Box::new(f)),
            shared: Arc::new(FiberShared {
                status: Mutex::new(FiberStatus::WillRun),
                result: Mutex::new(None),
                fiber_wakeup: Condvar::new(),
                executor_wakeup: Condvar::new(),
            }),
            thread: None,
        }
    }
}

impl<R: Send + 'static> std::future::Future for FiberFuture<R> {
    type Output = R;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<R> {
        // FiberFuture is Unpin (all fields are Unpin), so get_mut is safe.
        let this = self.get_mut();

        // Spawn the OS thread on the first poll.
        if this.thread.is_none() {
            let shared = this.shared.clone();
            let f = this.f.take().expect("FiberFuture polled after completion");
            this.thread = Some(std::thread::spawn(move || run_fiber(shared, f)));
        }

        loop {
            let mut status = this.shared.status.lock().unwrap();
            match *status {
                FiberStatus::WillRun => {
                    // Transition to Running and unpark the fiber thread.
                    *status = FiberStatus::Running;
                    this.shared.fiber_wakeup.notify_one();
                    // Block the executor thread until the fiber parks or finishes.
                    // This is intentional: Bach is single-threaded, so we must
                    // wait for the fiber to yield before proceeding.
                    let _guard = this
                        .shared
                        .executor_wakeup
                        .wait_while(status, |s| matches!(s, FiberStatus::Running))
                        .unwrap();
                    // Loop to process the new status (Parked or Done).
                }

                FiberStatus::Running => {
                    // Should not happen under correct usage.
                    drop(status);
                    return Poll::Pending;
                }

                FiberStatus::Parked(op) => {
                    // Fiber is waiting for a coop operation.  Ask the
                    // scheduler whether it may proceed.
                    drop(status);
                    match op.poll_acquire(cx) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(()) => {
                            // Coop granted — mark WillRun and loop to unpark.
                            *this.shared.status.lock().unwrap() = FiberStatus::WillRun;
                        }
                    }
                }

                FiberStatus::Done => {
                    drop(status);
                    let result = this
                        .shared
                        .result
                        .lock()
                        .unwrap()
                        .take()
                        .expect("FiberFuture result already taken");
                    return match result {
                        Ok(v) => Poll::Ready(v),
                        Err(e) => std::panic::resume_unwind(e),
                    };
                }
            }
        }
    }
}

/// Spawn a synchronous closure as a fiber task and return its [`JoinHandle`].
///
/// Inside the closure, [`Mutex::blocking_lock`](crate::sync::Mutex::blocking_lock)
/// suspends the OS thread at the call site instead of unwinding or blocking
/// the executor thread, so it is safe to call from ordinary (non-`async`) code.
///
/// # Example
///
/// ```ignore
/// bach::task::spawn_fiber(|| {
///     let mut guard = my_mutex.blocking_lock();
///     *guard += 1;
/// }).primary().spawn();
/// ```
pub fn spawn_fiber<F, R>(f: F) -> crate::task::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    crate::task::spawn(FiberFuture::new(f))
}
