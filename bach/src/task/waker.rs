use super::supervisor::{self, Events, Runnable, TaskId};
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

    pub fn check_contract(self: &Arc<Self>, runnable: &dyn Runnable) {
        let has_references = Arc::strong_count(&self) > 2;

        let in_run_queue = self.woken.load(Ordering::Relaxed);

        if !(has_references || in_run_queue) {
            let type_name = runnable.type_name();
            panic!(
                "\nWaker contract violation.\n\nFuture: {type_name}\n\n{}",
                concat!(
                    "The task has no active `Waker` references and returned `Poll::Pending`. ",
                    "It cannot be woken again and has likely leaked any associated memory.\n"
                )
            );
        }
    }

    pub fn after_poll(&self) {
        self.woken.store(false, Ordering::Relaxed);
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
