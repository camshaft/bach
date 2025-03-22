use super::supervisor::{Event, Events, TaskId};
use crate::sync::queue::Shared as _;
use std::{
    self,
    future::Future,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

#[derive(Debug)]
pub struct JoinError {
    _todo: (),
}

pub struct JoinHandle<T> {
    pub(super) state: Arc<State<T>>,
    events: Events,
}

impl<T> JoinHandle<T> {
    pub(super) fn new(events: Events) -> Self {
        let state = Default::default();
        Self { state, events }
    }

    pub fn abort(&self) {
        if let Some(task_id) = self.state.abort() {
            let _ = self.events.push(Event::Abort(task_id));
        }
    }

    pub fn is_finished(&self) -> bool {
        !matches!(self.state.status(), Status::Running)
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = Result<T, JoinError>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        self.state.poll(cx)
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Status {
    Running,
    Finished,
    Aborted,
}

impl Status {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }
}

pub(super) struct State<T> {
    output: Mutex<Output<T>>,
}

impl<T> Default for State<T> {
    fn default() -> Self {
        Self {
            output: Mutex::new(Output {
                status: Status::Running,
                v: None,
                waker: None,
                id: None,
            }),
        }
    }
}

impl<T> State<T> {
    pub fn set_id(&self, id: TaskId) {
        let Ok(mut output) = self.output.lock() else {
            return;
        };
        output.id = Some(id);
    }

    pub fn finish(&self, v: Option<T>) {
        let Ok(mut output) = self.output.lock() else {
            return;
        };
        output.status = Status::Finished;
        output.v = v;
        if let Some(waker) = output.waker.take() {
            drop(output);
            waker.wake();
        }
    }

    fn abort(&self) -> Option<TaskId> {
        let Ok(mut output) = self.output.lock() else {
            return None;
        };
        output.status = Status::Aborted;
        let _waker = output.waker.take();
        let _value = output.v.take();
        let id = output.id.take();
        drop(output);
        id
    }

    fn poll(&self, cx: &mut Context) -> Poll<Result<T, JoinError>> {
        let status = self.status();
        let Ok(mut output) = self.output.lock() else {
            return Err(JoinError { _todo: () }).into();
        };

        if !matches!(status, Status::Running) {
            return if let Some(v) = output.v.take() {
                Ok(v)
            } else {
                Err(JoinError { _todo: () })
            }
            .into();
        }

        output.waker = Some(cx.waker().clone());

        Poll::Pending
    }

    pub fn status(&self) -> Status {
        self.output.lock().map_or(Status::Aborted, |o| o.status)
    }
}

struct Output<T> {
    status: Status,
    v: Option<T>,
    waker: Option<std::task::Waker>,
    id: Option<TaskId>,
}
