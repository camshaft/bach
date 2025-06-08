use super::supervisor::{self, Events, TaskId};
use crate::{sync::queue::Shared as _, task::Info};
use std::{
    backtrace::Backtrace,
    mem::ManuallyDrop,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    task::{RawWaker, RawWakerVTable, Wake, Waker},
};

mod noop {
    use core::task::{RawWaker, RawWakerVTable, Waker};

    const VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop_cb, noop_cb, noop_cb);

    unsafe fn clone(ptr: *const ()) -> RawWaker {
        RawWaker::new(ptr, &VTABLE)
    }

    unsafe fn noop_cb(_ptr: *const ()) {
        // noop
    }

    pub fn noop() -> Waker {
        // TODO use `Waker::noop` once MSRV is 1.85.0
        unsafe { Waker::from_raw(clone(core::ptr::null())) }
    }
}

pub use noop::noop;

// BacktraceWaker implementation for capturing stack traces
// during deadlock detection

/// A waker that captures backtraces when cloned
pub struct DiagnosticWaker {
    // The underlying waker to delegate actual wake operations to
    real_waker: Waker,
    // The backtrace captured when this waker was cloned
    captured: Mutex<Option<(Info, Backtrace)>>,
}

impl DiagnosticWaker {
    /// Creates a new BacktraceWaker wrapping the given waker
    pub fn new(waker: Waker) -> Self {
        Self {
            real_waker: waker,
            captured: Mutex::new(None),
        }
    }

    /// Get the captured backtrace
    pub fn take(&self) -> Option<(Info, Backtrace)> {
        self.captured.lock().unwrap().take()
    }

    /// Creates a waker that captures backtraces when cloned
    pub fn into_waker(self: Arc<Self>) -> Waker {
        let data = Arc::into_raw(self) as *const ();
        unsafe { Waker::from_raw(RawWaker::new(data, Self::waker_vtable())) }
    }

    // RawWaker vtable implementation
    fn waker_vtable() -> &'static RawWakerVTable {
        &RawWakerVTable::new(
            Self::clone_raw,
            Self::wake_raw,
            Self::wake_by_ref_raw,
            Self::drop_raw,
        )
    }

    /// Clone implementation that captures a backtrace when the waker is cloned
    unsafe fn clone_raw(ptr: *const ()) -> RawWaker {
        let this = Arc::from_raw(ptr as *const Self);
        let this = ManuallyDrop::new(this);

        // Capture a backtrace when the waker is cloned
        // This happens when a task suspends (Poll::Pending) and registers its waker
        // with some resource it's waiting on
        if let Ok(mut guard) = this.captured.lock() {
            if guard.is_none() {
                let info = crate::task::Info::current();
                *guard = Some((info, Backtrace::force_capture()));
            }
        }

        Arc::increment_strong_count(ptr);

        RawWaker::new(ptr, Self::waker_vtable())
    }

    /// Wake implementation that delegates to the real waker
    unsafe fn wake_raw(ptr: *const ()) {
        let this = Arc::from_raw(ptr as *mut Self);
        this.real_waker.wake_by_ref();
    }

    /// Wake by reference implementation that delegates to the real waker
    unsafe fn wake_by_ref_raw(ptr: *const ()) {
        let this = &*(ptr as *const Self);
        this.real_waker.wake_by_ref();
    }

    /// Drop implementation for cleaning up the waker
    unsafe fn drop_raw(ptr: *const ()) {
        drop(Arc::from_raw(ptr as *mut Self));
    }
}

pub struct ForTask {
    idx: TaskId,
    woken: AtomicBool,
    events: Events,
}

impl ForTask {
    pub fn new(idx: TaskId, events: Events) -> Self {
        Self {
            idx,
            woken: AtomicBool::new(false),
            events,
        }
    }

    pub fn status(self: &Arc<Self>) -> Status {
        let has_references = Arc::strong_count(self) > 2;
        let in_run_queue = self.woken.load(Ordering::Relaxed);

        Status {
            in_run_queue,
            has_references,
        }
    }

    pub fn before_poll(&self) {
        self.woken.store(false, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Status {
    pub in_run_queue: bool,
    pub has_references: bool,
}

impl Status {
    pub fn is_zombie(&self) -> bool {
        !self.in_run_queue && !self.has_references
    }
}

impl Wake for ForTask {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let was_woken = self.woken.swap(true, Ordering::Relaxed);
        if !was_woken {
            let _ = self.events.push(supervisor::Event::Run(self.idx));
        }
    }
}
