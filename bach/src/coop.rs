use crate::{define, ext::*};
use std::{
    collections::{btree_map::Entry, BTreeMap},
    future::poll_fn,
    sync::{Arc, Mutex},
    task::{ready, Context, Poll, Waker},
};

define!(scope, Coop);

trait WakerExt {
    fn id(&self) -> usize;
}

impl WakerExt for Waker {
    fn id(&self) -> usize {
        #[cfg(feature_waker_data)]
        {
            self.data() as usize
        }
        #[cfg(not(feature_waker_data))]
        {
            // Use the task ID instead of the waker pointer.
            // This may lead to issues when the same task is using different wakers
            // but should be rare. The `DisjointSet` impl has checks to prevent the
            // execution from continuing so it should be safe.
            crate::task::Info::current().id() as _
        }
    }
}

mod disjoint_set;

#[derive(Clone, Default)]
pub struct Coop(Arc<Mutex<State>>);

#[derive(Default)]
struct State {
    id: u64,
    set: disjoint_set::DisjointSet,
    status: BTreeMap<(usize, Operation), Poll<()>>,
    moves: Vec<usize>,
}

impl State {
    fn schedule(&mut self) -> usize {
        let max_group_size = self.set.max_group_size() as usize;

        measure!("max_group_size", max_group_size as f64);

        if max_group_size == 0 {
            if cfg!(test) {
                assert!(self.status.iter().all(|(_id, status)| status.is_ready()));
            }
            self.status.clear();
            return 0;
        }

        // Generate a set of interleavings from the `max_depth` value
        //
        // We generate this once with the assumption that each operation
        // interleaving is independent from one another. Doing so can drastically
        // cut down on the required search space.
        // See: https://en.wikipedia.org/wiki/Partial_order_reduction
        self.moves.clear();
        let max_dst = max_group_size.saturating_sub(1);
        for src in 0..max_dst {
            let dst = (src..=max_dst).any();
            self.moves.push(dst);
        }

        for status in self.status.values_mut() {
            *status = Poll::Ready(());
        }

        self.set.schedule(|tasks| {
            if cfg!(test) {
                assert!((2..=(self.moves.len() + 1)).contains(&tasks.len()));
            }

            for (src, dst) in self.moves.iter().copied().enumerate() {
                // return is the moves exceed the number of tasks for this group
                if src == tasks.len() {
                    break;
                }

                // if dst is in-bounds, then swap it with src. otherwise, leave it in place
                if dst < tasks.len() {
                    tasks.swap(src, dst);
                }
            }

            // wake all of the tasks after applying the moves
            for waker in tasks.drain(..) {
                waker.wake();
            }
        })
    }

    fn poll_acquire(&mut self, cx: &mut Context, operation: Operation) -> Poll<()> {
        let waker = cx.waker();

        let waker_id = waker.id();

        let key = (waker_id, operation);
        match self.status.entry(key) {
            Entry::Vacant(entry) => {
                entry.insert(Poll::Pending);

                count!("pending", "operation" = operation.0.to_string());

                self.set.join(waker, waker_id, operation.0);
                Poll::Pending
            }
            Entry::Occupied(entry) => {
                ready!(*entry.get());

                count!("ready", "operation" = operation.0.to_string());

                entry.remove()
            }
        }
    }
}

impl Coop {
    pub fn enter<F: FnOnce() -> R, R>(&self, f: F) -> R {
        let (_, res) = scope::with(self.clone(), f);
        res
    }

    pub fn schedule(&self) -> usize {
        self.0.lock().unwrap().schedule()
    }

    fn resource(&mut self) -> Operation {
        let mut state = self.0.lock().unwrap();
        let id = state.id;
        state.id += 1;
        Operation(id)
    }

    fn poll_acquire(&mut self, cx: &mut Context<'_>, operation: Operation) -> Poll<()> {
        self.0.lock().unwrap().poll_acquire(cx, operation)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Operation(u64);

impl Operation {
    pub fn register() -> Self {
        if cfg!(not(feature = "coop")) {
            return Self(u64::MAX);
        }

        scope::try_borrow_mut_with(|coop| coop.as_mut().map(|coop| coop.resource()))
            .unwrap_or(Self(u64::MAX))
    }

    pub async fn acquire(&self) {
        if cfg!(not(feature = "coop")) {
            return;
        }

        poll_fn(|cx| self.poll_acquire(cx)).await
    }

    pub fn poll_acquire(&self, cx: &mut Context<'_>) -> Poll<()> {
        if cfg!(not(feature = "coop")) {
            return Poll::Ready(());
        }

        scope::try_borrow_mut_with(|coop| coop.as_mut().map(|coop| coop.poll_acquire(cx, *self)))
            .unwrap_or(Poll::Ready(()))
    }
}
