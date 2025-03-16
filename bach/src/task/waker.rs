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
