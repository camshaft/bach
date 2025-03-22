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
        self.supervisor.microstep()
    }

    pub fn macrostep(&mut self) -> Macrostep {
        let mut total = 0;
        let mut steps = 0;
        loop {
            let tasks = self.environment.enter(|| self.supervisor.microstep());

            // loop until all of the tasks have settled
            if tasks != 0 {
                total += tasks;
                steps += 1;

                if let Some(max) = self.max_microsteps {
                    if steps > max {
                        panic!(
                            "\nTask contract violation.\n\n{}{}{}",
                            "The runtime has exceeded the configured `max_microsteps` limit of ",
                            max,
                            concat!(
                                ". This is likely due to a bug in the application that prevents time ",
                                "moving forward by continually waking tasks. Enable the `tracing` and ",
                                "`metrics` feature in `bach` to identify which ",
                                "task(s) are causing this issue."
                            )
                        );
                    }
                }

                continue;
            }

            let macrostep = Macrostep {
                tasks: total,
                ticks: 0,
            };
            let macrostep = self.environment.on_macrostep(macrostep);

            self.environment.enter(|| macrostep.metrics());

            return macrostep;
        }
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

    pub fn primary_count(&self) -> u64 {
        self.handle.primary_count()
    }

    pub fn block_on_primary(&mut self) {
        loop {
            self.macrostep();

            if self.handle.primary_count() == 0 {
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn executor() -> Executor<Env> {
        Executor::new(|_| Env)
    }

    #[derive(Default)]
    pub struct Env;

    impl super::Environment for Env {
        fn enter<F: FnOnce() -> O, O>(&mut self, f: F) -> O {
            f()
        }

        fn run<T, R>(&mut self, _tasks: T) -> Poll<()>
        where
            T: IntoIterator<Item = R>,
            R: crate::environment::Runnable,
        {
            unimplemented!()
        }
    }

    #[derive(Default)]
    struct Yield(bool);

    impl Future for Yield {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            if core::mem::replace(&mut self.0, true) {
                Poll::Ready(())
            } else {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    /*
    #[test]
    fn basic_test() {
        let mut executor = executor();

        let queue = Arc::new(queue::vec_deque::Queue::default().mutex());

        crate::task::scope::with(executor.handle().clone(), || {
            use crate::task::spawn;

            let s1 = queue.clone();
            spawn(async move {
                Yield::default().await;
                let _ = s1.push_lazy(&mut Some("hello"));
                Yield::default().await;
            });

            let s2 = queue.clone();
            let exclaimation = async move {
                Yield::default().await;
                let _ = s2.push_lazy(&mut Some("!!!!!"));
                Yield::default().await;
            };

            let s3 = queue.clone();
            spawn(async move {
                Yield::default().await;
                let _ = s3.push_lazy(&mut Some("world"));
                Yield::default().await;
                exclaimation.await;
                Yield::default().await;
            });
        });

        executor.macrostep();

        let mut output = String::new();
        for chunk in queue.lock().unwrap().drain() {
            output.push_str(chunk);
        }

        assert_eq!(output, "helloworld!!!!!");
    }
    */
}
