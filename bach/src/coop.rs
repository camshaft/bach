use crate::{define, task::Info, time::Instant};
use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Waker},
};

define!(scope, Coop);

pub struct Coop {
    id: u64,
    operations: BTreeMap<Operation, VecDeque<Task>>,
}

impl Coop {
    pub fn resource(&mut self) -> Operation {
        let id = self.id;
        self.id += 1;
        Operation(id)
    }

    pub fn acquire(&mut self, cx: &mut Context<'_>, resource: &Operation) -> Waiting {
        let handle = Arc::new(());

        let task = Task {
            info: Info::current(),
            instant: Instant::now(),
            waker: cx.waker().clone(),
            handle: handle.clone(),
        };

        self.operations
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
    pub info: Info,
    pub instant: Instant,
    pub waker: Waker,

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
