use super::{
    info::WithInfo,
    join,
    supervisor::{Event, Events, Runnable, TaskId},
    JoinHandle,
};
use crate::{sync::queue::Shared, task::supervisor::RunOutcome};
use core::future::Future;
use pin_project_lite::pin_project;
use std::{pin::Pin, sync::Arc, task::Context};

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

    fn record_cost(self: Pin<&mut Self>, cost: core::time::Duration) {
        self.project().future.record_cost(cost);
    }

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> RunOutcome {
        let this = self.project();
        if !this.output.status().is_running() {
            return RunOutcome::Done(());
        }
        match this.future.poll(cx) {
            RunOutcome::Done(value) => {
                this.output.finish(Some(value));
                RunOutcome::Done(())
            }
            RunOutcome::PayingDebt => RunOutcome::PayingDebt,
            RunOutcome::ExecutedApplication => RunOutcome::ExecutedApplication,
        }
    }
}
