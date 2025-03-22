use crate::{
    environment::{Environment, Macrostep},
    task::supervisor::{Events, Supervisor},
};
use alloc::sync::Arc;
use core::{
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicU64, Ordering},
    task::{Context, Poll},
};

pub use crate::task::JoinHandle;

pub struct Executor<E: Environment> {
    environment: E,
    handle: Handle,
    supervisor: Supervisor,
    max_microsteps: Option<usize>,
}

impl<E: Environment> Executor<E> {
    pub fn new<F: FnOnce(&Handle) -> E>(create_env: F) -> Self {
        let supervisor = Supervisor::default();

        let handle = Handle {
            events: supervisor.events(),
            primary_count: Default::default(),
            ids: Default::default(),
            task_counts: supervisor.task_counts(),
        };

        let environment = create_env(&handle);

        Self {
            environment,
            handle,
            supervisor,
            max_microsteps: Some(100_000),
        }
    }

    /// Sets the maximum number of times that a task can wake itself up after polling
    pub fn set_max_self_wakes(&mut self, max: Option<usize>) {
        self.supervisor.set_max_self_wakes(max);
    }

    pub fn spawn<F, Output>(&self, future: F) -> JoinHandle<Output>
    where
        F: Future<Output = Output> + Send + 'static,
        Output: Send + 'static,
    {
        self.handle.spawn(future)
    }

    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    pub fn microstep(&mut self) -> usize {
        self.environment.enter(|| {
            let ticks = crate::time::scheduler::ticks();
            self.supervisor.microstep(ticks)
        })
    }

    pub fn macrostep(&mut self) -> Macrostep {
        self.macrostep_inner(false)
    }

    fn macrostep_inner(&mut self, stop_at_zero_primary: bool) -> Macrostep {
        let mut total = 0;
        let mut steps = 0;

        let mut is_ok = true;

        self.environment.enter(|| {
            loop {
                let ticks = crate::time::scheduler::ticks();
                let tasks = self.supervisor.microstep(ticks);

                // all of the pending tasks have settled
                if tasks == 0 {
                    break;
                }

                total += tasks;
                steps += 1;

                // check if we're still in bounds
                if let Some(max) = self.max_microsteps {
                    if steps > max {
                        is_ok = false;
                        break;
                    }
                }
            }
        });

        if !is_ok {
            panic!(
                "\nTask contract violation.\n\n{}{}{}",
                "The runtime has exceeded the configured `max_microsteps` limit of ",
                self.max_microsteps.unwrap(),
                concat!(
                    ". This is likely due to a bug in the application that prevents time ",
                    "moving forward by continually waking tasks. Enable the `tracing` and ",
                    "`metrics` feature in `bach` to identify which ",
                    "task(s) are causing this issue."
                )
            );
        }

        let macrostep = Macrostep {
            tasks: total,
            ticks: 0,
            primary_count: self.handle.primary_count(),
        };

        if stop_at_zero_primary && macrostep.primary_count == 0 {
            return macrostep;
        }

        let macrostep = self.environment.on_macrostep(macrostep);

        #[cfg(feature = "metrics")]
        self.environment.enter(|| macrostep.metrics());

        macrostep
    }

    pub fn block_on<T, Output>(&mut self, task: T) -> Output
    where
        T: 'static + Future<Output = Output> + Send,
        Output: 'static + Send,
    {
        let mut task = self.spawn(task);
        let waker = crate::task::waker::noop();
        let mut ctx = Context::from_waker(&waker);

        loop {
            self.macrostep();

            if let Poll::Ready(value) = Pin::new(&mut task).poll(&mut ctx) {
                return value.expect("task did not complete");
            }
        }
    }

    pub fn block_on_primary(&mut self) {
        loop {
            // Don't call `on_macrostep` once `primary` hit `0`. This avoids incrementing
            // the clock to a non-primary time.
            let result = self.macrostep_inner(true);

            if result.primary_count == 0 {
                return;
            }
        }
    }

    pub fn environment(&mut self) -> &mut E {
        &mut self.environment
    }

    pub fn close(&mut self) {
        if std::thread::panicking() {
            return;
        }

        let closer = self.supervisor.close();
        self.environment.close(move || {
            drop(closer);
        });
    }
}

impl<E: Environment> Drop for Executor<E> {
    fn drop(&mut self) {
        self.close();
    }
}

#[derive(Clone)]
pub struct Handle {
    events: Events,
    primary_count: Arc<AtomicU64>,
    ids: Arc<AtomicU64>,
    task_counts: Arc<AtomicU64>,
}

impl Handle {
    pub fn current() -> Self {
        crate::task::scope::borrow_with(|handle| handle.clone())
    }

    pub fn spawn<F, Output>(&self, future: F) -> JoinHandle<Output>
    where
        F: Future<Output = Output> + Send + 'static,
        Output: Send + 'static,
    {
        self.spawn_named(future, "")
    }

    pub fn spawn_named<F, N, Output>(&self, future: F, name: N) -> JoinHandle<Output>
    where
        F: Future<Output = Output> + Send + 'static,
        Output: Send + 'static,
        N: core::fmt::Display,
    {
        count!("spawn");

        let id = self.ids.fetch_add(1, Ordering::Relaxed);
        let name = Arc::from(name.to_string());

        let future = crate::task::info::WithInfo::new(future, id, &name);

        crate::task::spawn::event(&self.events, future)
    }

    pub fn enter<F: FnOnce() -> O, O>(&self, f: F) -> O {
        let (_, res) = crate::task::scope::with(self.clone(), f);
        res
    }

    pub fn primary_guard(&self) -> crate::task::primary::Guard {
        crate::task::primary::Guard::new(self.primary_count.clone())
    }

    pub fn snapshot(&self) -> Snapshot {
        let groups = crate::group::list();
        Snapshot {
            primary_count: self.primary_count(),
            tasks: self.task_counts.load(Ordering::Relaxed),
            groups,
        }
    }

    fn primary_count(&self) -> u64 {
        self.primary_count.load(Ordering::SeqCst)
    }
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Snapshot {
    pub primary_count: u64,
    pub tasks: u64,
    pub groups: Vec<crate::group::Group>,
}
