use super::{
    info::WithInfo,
    join,
    supervisor::{Event, Events, Runnable, TaskId},
    JoinHandle,
};
use crate::sync::queue::Shared;
use core::future::Future;
use pin_project_lite::pin_project;
use std::{
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
};

pub fn event<F>(events: &Events, future: WithInfo<F>, internal: bool) -> JoinHandle<F::Output>
where
    F: 'static + Future,
    F::Output: 'static,
{
    spawn_future(events, future, internal)
}

pub fn internal_event<F>(events: &Events, future: F) -> JoinHandle<F::Output>
where
    F: 'static + Future,
    F::Output: 'static,
{
    spawn_future(events, future, true)
}

fn spawn_future<G>(events: &Events, future: G, internal: bool) -> JoinHandle<G::Output>
where
    G: 'static + Future,
    G::Output: 'static,
{
    let handle = JoinHandle::new(events.clone());
    let future = TaskFuture {
        future,
        output: handle.state.clone(),
    };
    let future = Box::pin(future);
    if events.push(Event::Spawn(future, internal)).is_err() {
        handle.state.finish(None);
    }
    handle
}

pin_project! {
    pub struct TaskFuture<G> where G: Future {
        #[pin]
        future: G,

        output: Arc<join::State<G::Output>>,
    }
}

impl<G> Runnable for TaskFuture<G>
where
    G: 'static + Future,
    G::Output: 'static,
{
    fn type_name(&self) -> &'static str {
        std::any::type_name::<G>()
    }

    fn set_id(self: Pin<&mut Self>, id: TaskId) {
        let this = self.project();
        this.output.set_id(id);
    }

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<()> {
        let this = self.project();
        if !this.output.status().is_running() {
            return Poll::Ready(());
        }
        let value = ready!(this.future.poll(cx));
        this.output.finish(Some(value));
        Poll::Ready(())
    }
}
