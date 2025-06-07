use super::waker;
use crate::{
    queue::{self, QueueExt as _},
    task,
};
use core::fmt;
use slotmap::{new_key_type, SlotMap};
use std::{
    backtrace::Backtrace,
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

pub struct Closer(#[allow(dead_code)] Inner);

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

    pub fn events(&self) -> Events {
        self.0.events.clone()
    }

    pub fn microstep(&mut self, current_tick: u64) -> usize {
        self.0.microstep(current_tick)
    }

    /// Collect diagnostic information about all tasks
    pub fn diagnostics(&mut self) -> Vec<TaskDiagnostics> {
        self.0.diagnostics()
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
    fn diagnostics(&mut self) -> Vec<TaskDiagnostics> {
        let mut diagnostics = Vec::new();

        for (_task_id, task) in self.tasks.iter_mut() {
            diagnostics.push(task.diagnostic());
        }

        diagnostics
    }

    fn microstep(&mut self, current_tick: u64) -> usize {
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
                                self_wakes: Default::default(),
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
                        let res = slot.poll(current_tick, &self.max_self_wakes);
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

/// Diagnostic information about a task
#[derive(Debug)]
pub struct TaskDiagnostics {
    /// Type name of the task
    pub type_name: &'static str,
    /// Information about the task
    pub info: Option<task::Info>,
    /// A backtrace captured during the task's most recent poll
    pub backtrace: Option<Backtrace>,
}

impl fmt::Display for TaskDiagnostics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(info) = &self.info {
            if let Some(name) = info.name() {
                writeln!(f, "Task: {name}")?;
                writeln!(f, "  Type: {}", self.type_name)?;
            } else {
                writeln!(f, "Task: {}", self.type_name)?;
            }
        } else {
            writeln!(f, "Task: {}", self.type_name)?;
        }

        if let Some(backtrace) = &self.backtrace {
            writeln!(f, "\n  Backtrace:\n{}", backtrace)?;
        }

        Ok(())
    }
}

struct Slot {
    waker_state: Arc<waker::ForTask>,
    waker: Waker,
    runnable: DynRunnable,
    self_wakes: SelfWakes,
}

impl Slot {
    fn poll(&mut self, current_tick: u64, max_self_wakes: &Option<usize>) -> Poll<()> {
        self.waker_state.before_poll();

        let cx = &mut Context::from_waker(&self.waker);
        let res = self.runnable.as_mut().poll(cx);

        // check that the task contract is enforced
        if cfg!(debug_assertions) && res.is_pending() {
            self.check_status(current_tick, max_self_wakes);
        }

        res
    }

    fn diagnostic(&mut self) -> TaskDiagnostics {
        self.waker_state.before_poll();

        // Create a BacktraceWaker that will capture stack traces when cloned
        let backtrace_waker = Arc::new(super::waker::DiagnosticWaker::new(self.waker.clone()));
        let waker = backtrace_waker.clone().into_waker();

        // Create a context with our waker
        let cx = &mut Context::from_waker(&waker);

        // Poll the runnable, which may cause the waker to be cloned if the task is pending
        // and is waiting on some resource
        let _ = self.runnable.as_mut().poll(cx);

        let (info, backtrace) = if let Some((info, backtrace)) = backtrace_waker.take() {
            (Some(info), Some(backtrace))
        } else {
            (None, None)
        };

        TaskDiagnostics {
            type_name: self.runnable.type_name(),
            info,
            backtrace,
        }
    }

    fn check_status(&mut self, current_tick: u64, max_self_wakes: &Option<usize>) {
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
            if !self.self_wakes.can_self_wake(current_tick, max_self_wakes) {
                let type_name = self.runnable.as_ref().type_name();
                panic!(
                    "\nTask contract violation.\n\nFuture: {type_name}\n\n{}{}{}",
                    "The task has been self-woken more than `max_self_wakes` (",
                    max_self_wakes.unwrap(),
                    ") times. This is likely a bug in the application's task implementation.\n"
                );
            }
        } else {
            self.self_wakes.reset(current_tick);
        }
    }
}

#[derive(Debug, Default)]
struct SelfWakes {
    count: usize,
    last_update: u64,
}

impl SelfWakes {
    fn can_self_wake(&mut self, current_tick: u64, max: &Option<usize>) -> bool {
        if self.last_update < current_tick {
            self.reset(current_tick);
            return true;
        }

        self.count += 1;

        let Some(max) = *max else {
            return true;
        };

        if self.count <= max {
            return true;
        }

        false
    }

    fn reset(&mut self, current_tick: u64) {
        self.count = 0;
        self.last_update = current_tick;
    }
}
