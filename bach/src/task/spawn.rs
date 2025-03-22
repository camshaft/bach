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

pub fn event<F>(events: &Events, future: WithInfo<F>) -> JoinHandle<F::Output>
where
    F: 'static + Future + Send,
    F::Output: 'static + Send,
{
    let handle = JoinHandle::new(events.clone());
    let future = TaskFuture {
        future,
        output: handle.state.clone(),
    };
    let future = Box::pin(future);
    if events.push(Event::Spawn(future)).is_err() {
        handle.state.finish(None);
    }
    handle
}

pin_project! {
    pub struct TaskFuture<F> where F: Future {
        #[pin]
        future: WithInfo<F>,

        output: Arc<join::State<F::Output>>,
    }
}

impl<F> Runnable for TaskFuture<F>
where
    F: 'static + Future + Send,
    F::Output: 'static + Send,
{
    fn type_name(&self) -> &'static str {
        std::any::type_name::<F>()
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
