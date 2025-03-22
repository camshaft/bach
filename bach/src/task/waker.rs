use super::supervisor::{self, Events, TaskId};
use crate::sync::queue::Shared as _;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::Wake,
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

    pub fn after_poll(&self) {
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
