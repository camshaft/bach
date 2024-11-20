pub trait Entry: Sized {
    type Queue: Queue<Self>;

    fn delay(&self) -> u64;
    fn start_tick(&self) -> u64;
    fn set_start_tick(&mut self, tick: u64);
}

pub trait Queue<Entry> {
    fn new() -> Self;
    fn is_empty(&self) -> bool;
    fn push(&mut self, entry: Entry);
    fn pop(&mut self) -> Option<Entry>;
    fn take(&mut self) -> Self;
}

pub mod atomic {
    use super::*;
    use alloc::sync::Arc;
    use atomic_waker::AtomicWaker;
    use core::{
        sync::atomic::{AtomicBool, AtomicU64, Ordering},
        task::Waker,
    };
    use intrusive_collections::{intrusive_adapter, LinkedList, LinkedListLink};

    intrusive_adapter!(pub Adapter = ArcEntry: Entry { link: LinkedListLink });

    pub type ArcEntry = Arc<Entry>;

    #[derive(Debug)]
    pub struct Entry {
        waker: AtomicWaker,
        expired: AtomicBool,
        registered: AtomicBool,
        delay: u64,
        start_tick: AtomicU64,
        link: LinkedListLink,
    }

    unsafe impl Send for Entry {}
    unsafe impl Sync for Entry {}

    pub fn wake(entry: ArcEntry) {
        entry.wake();
    }

    impl Entry {
        pub fn new(delay: u64) -> Arc<Self> {
            Arc::new(Self {
                waker: AtomicWaker::new(),
                expired: AtomicBool::new(false),
                registered: AtomicBool::new(false),
                delay,
                start_tick: AtomicU64::new(0),
                link: LinkedListLink::new(),
            })
        }

        pub fn wake(&self) {
            self.expired.store(true, Ordering::SeqCst);
            self.registered.store(false, Ordering::SeqCst);

            if let Some(waker) = self.waker.take() {
                waker.wake();
            }
        }

        pub fn should_register(&self) -> bool {
            !self.registered.swap(true, Ordering::SeqCst)
        }

        pub fn cancel(&self) {
            self.waker.take();
        }

        pub fn take_expired(&self) -> bool {
            self.expired.swap(false, Ordering::SeqCst)
        }

        pub fn register(&self, waker: &Waker) {
            self.waker.register(waker)
        }
    }

    impl super::Entry for Arc<Entry> {
        type Queue = LinkedList<Adapter>;

        fn delay(&self) -> u64 {
            self.delay
        }

        fn start_tick(&self) -> u64 {
            self.start_tick.load(Ordering::SeqCst)
        }

        fn set_start_tick(&mut self, tick: u64) {
            self.start_tick.store(tick, Ordering::SeqCst);
        }
    }

    impl Drop for Entry {
        fn drop(&mut self) {
            self.cancel();
        }
    }

    impl Queue<ArcEntry> for LinkedList<Adapter> {
        fn new() -> Self {
            LinkedList::new(Adapter::new())
        }

        fn is_empty(&self) -> bool {
            LinkedList::is_empty(self)
        }

        fn push(&mut self, entry: ArcEntry) {
            self.push_back(entry);
        }

        fn pop(&mut self) -> Option<ArcEntry> {
            self.pop_front()
        }

        fn take(&mut self) -> Self {
            LinkedList::take(self)
        }
    }
}
