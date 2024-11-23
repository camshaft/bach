use crate::{define, ext::*};
use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};

define!(scope, Coop);

#[derive(Clone, Default)]
pub struct Coop(Arc<Mutex<State>>);

#[derive(Default)]
struct State {
    id: u64,
    operations: BTreeMap<Operation, VecDeque<Task>>,
    moves: Vec<usize>,
}

impl State {
    fn schedule(&mut self) -> usize {
        let mut woken_tasks = 0;
        let mut max_len = 0;

        // First look at all of the pending tasks and find the `max_len`
        for tasks in self.operations.values() {
            woken_tasks += tasks.len();
            max_len = max_len.max(tasks.len());
        }

        // Generate a set of interleavings from the `max_len` value
        //
        // We generate this once with the assumption that each operation
        // interleaving is independent from one another. Doing so can drastically
        // cut down on the required search space.
        // See: https://en.wikipedia.org/wiki/Partial_order_reduction
        self.moves.clear();
        let max_dst = max_len.saturating_sub(1);
        for src in 0..max_dst {
            let dst = (src..=max_dst).any();
            self.moves.push(dst);
        }

        self.operations.retain(|_operation, tasks| {
            for (src, dst) in self.moves.iter().copied().enumerate() {
                // make sure the src applies to this set of tasks
                if src == tasks.len() {
                    break;
                }

                // if dst is in-bounds, then swap it with src. otherwise, leave it in place
                if dst < tasks.len() {
                    tasks.swap(src, dst);
                }
            }

            for task in tasks.drain(..) {
                // dropping it wakes it up
                drop(task)
            }

            // clear out everything
            false
        });

        woken_tasks
    }
}

impl Coop {
    pub fn enter<F: FnOnce() -> R, R>(&self, f: F) -> R {
        scope::with(self.clone(), f)
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

    fn acquire(&mut self, cx: &mut Context<'_>, resource: &Operation) -> Waiting {
        let handle = Arc::new(());

        let task = Task {
            waker: cx.waker().clone(),
            handle: handle.clone(),
        };

        self.0
            .lock()
            .unwrap()
            .operations
            .entry(*resource)
            .or_default()
            .push_back(task);

        Waiting { handle }
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
            .unwrap_or(Operation(u64::MAX))
    }

    pub async fn acquire(&self) {
        if cfg!(not(feature = "coop")) {
            return;
        }

        let Some(future) = core::future::poll_fn(|cx| {
            Poll::Ready(scope::try_borrow_mut_with(|coop| {
                coop.as_mut().map(|coop| coop.acquire(cx, self))
            }))
        })
        .await
        else {
            return;
        };

        future.await;
    }
}

pub struct Task {
    waker: Waker,
    #[allow(dead_code)] // this just holds the `Waiting` future open
    handle: Arc<()>,
}

impl Drop for Task {
    fn drop(&mut self) {
        self.waker.wake_by_ref()
    }
}

#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Waiting {
    handle: Arc<()>,
}

impl Future for Waiting {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if Arc::strong_count(&self.handle) > 1 {
            count!("yield");
            return Poll::Pending;
        }
        Poll::Ready(())
    }
}
