use super::waker;
use crate::queue::{self, QueueExt as _};
use core::fmt;
use slotmap::{new_key_type, SlotMap};
use std::{
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll, Waker},
};

pub type DynRunnable = Pin<Box<dyn Runnable>>;

#[cfg(feature = "metrics")]
type Q<T> = Arc<Mutex<queue::span::Queue<queue::vec_deque::Queue<T>>>>;
#[cfg(not(feature = "metrics"))]
type Q<T> = Arc<Mutex<queue::vec_deque::Queue<T>>>;

pub type Events = Q<Event>;

pub enum Event {
    Spawn(DynRunnable),
    Run(TaskId),
    Abort(TaskId),
}

impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Event::Spawn(_) => f.debug_tuple("Spawn").finish(),
            Event::Run(task_id) => f.debug_tuple("Run").field(task_id).finish(),
            Event::Abort(task_id) => f.debug_tuple("Abort").field(task_id).finish(),
        }
    }
}

pub trait Runnable: 'static + Send {
    fn type_name(&self) -> &'static str;

    fn set_id(self: Pin<&mut Self>, task_id: TaskId);

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<()>;
}

new_key_type! {
    pub struct TaskId;
}

type Tasks = SlotMap<TaskId, Slot>;

pub struct Closer(Inner);

impl Drop for Closer {
    fn drop(&mut self) {
        let _ = self.0.microstep();
    }
}

pub struct Supervisor(Inner);

impl Default for Supervisor {
    fn default() -> Self {
        let tasks = Default::default();
        let events = queue::vec_deque::Queue::builder()
            .with_capacity(None)
            .build();

        #[cfg(feature = "metrics")]
        let events = events.span("bach::events");

        let events = events.mutex();

        let events = Arc::new(events);
        let task_counts = Default::default();
        Self(Inner {
            tasks,
            events,
            task_counts,
            max_self_wakes: Some(100_000),
        })
    }
}

impl Supervisor {
    pub fn set_max_self_wakes(&mut self, max_self_wakes: Option<usize>) {
        self.0.max_self_wakes = max_self_wakes;
    }

    pub fn task_counts(&self) -> Arc<AtomicU64> {
        self.0.task_counts.clone()
    }

    pub fn events(&self) -> Events {
        self.0.events.clone()
    }

    pub fn microstep(&mut self) -> usize {
        self.0.microstep()
    }

    pub fn close(&mut self) -> Closer {
        Closer(Inner {
            tasks: std::mem::take(&mut self.0.tasks),
            events: self.0.events.clone(),
            task_counts: self.0.task_counts.clone(),
            max_self_wakes: self.0.max_self_wakes,
        })
    }
}

struct Inner {
    tasks: Tasks,
    events: Events,
    task_counts: Arc<AtomicU64>,
    max_self_wakes: Option<usize>,
}

impl Inner {
    fn microstep(&mut self) -> usize {
        let mut count = 0;
        let Ok(mut guard) = self.events.lock() else {
            return count;
        };

        // prevent tasks from injecting themselves over and over again in this microstep
        let mut events = guard.drain();
        drop(guard);

        while let Some(mut event) = events.pop_front() {
            loop {
                match event {
                    Event::Spawn(mut runnable) => {
                        let task_id = self.tasks.insert_with_key(|task_id| {
                            runnable.as_mut().set_id(task_id);
                            let run_queue = self.events.clone();
                            let waker_state = Arc::new(waker::ForTask::new(task_id, run_queue));
                            let waker: Waker = waker_state.clone().into();
                            waker.wake_by_ref();
                            Slot {
                                waker_state,
                                waker,
                                runnable,
                                self_wakes: 0,
                            }
                        });
                        self.task_counts.fetch_add(1, Ordering::Relaxed);
                        event = Event::Run(task_id);
                        continue;
                    }
                    Event::Run(task_id) => {
                        let Some(slot) = self.tasks.get_mut(task_id) else {
                            break;
                        };
                        let res = slot.poll(&self.max_self_wakes);
                        if res.is_ready() {
                            self.tasks.remove(task_id);
                            self.task_counts.fetch_sub(1, Ordering::Relaxed);
                        }
                        count += 1;
                    }
                    Event::Abort(task_id) => {
                        let res = self.tasks.remove(task_id);
                        if res.is_some() {
                            self.task_counts.fetch_sub(1, Ordering::Relaxed);
                            count += 1;
                        }
                    }
                }
                break;
            }
        }
        count
    }
}

struct Slot {
    waker_state: Arc<waker::ForTask>,
    waker: Waker,
    runnable: DynRunnable,
    self_wakes: usize,
}

impl Slot {
    fn poll(&mut self, max_self_wakes: &Option<usize>) -> Poll<()> {
        let cx = &mut Context::from_waker(&self.waker);
        let res = self.runnable.as_mut().poll(cx);

        // check that the task contract is enforced
        if res.is_pending() {
            self.check_status(max_self_wakes);
        }

        res
    }

    fn check_status(&mut self, max_self_wakes: &Option<usize>) {
        let status = self.waker_state.status();

        if status.is_zombie() {
            let type_name = self.runnable.as_ref().type_name();
            panic!(
                "\nTask contract violation.\n\nFuture: {type_name}\n\n{}",
                concat!(
                    "The task has no active `Waker` references and returned `Poll::Pending`. ",
                    "It cannot be woken again and has likely leaked any associated memory.\n"
                )
            );
        }

        if status.in_run_queue {
            self.self_wakes += 1;
            if let Some(max) = *max_self_wakes {
                if self.self_wakes > max {
                    let type_name = self.runnable.as_ref().type_name();
                    panic!(
                        "\nTask contract violation.\n\nFuture: {type_name}\n\n{}{}{}",
                        "The task has been self-woken more than `max_self_wakes` (",
                        max,
                        ") times. This is likely a bug in the application's task implementation.\n"
                    );
                }
            }
        } else {
            self.self_wakes = 0;
        }

        self.waker_state.after_poll();
    }
}
