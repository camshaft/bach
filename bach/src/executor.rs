use crate::{
    environment::{Environment, Macrostep},
    sync::queue::{self, Queue as _},
};
use alloc::sync::Arc;
use async_task::{Runnable, Task};
use core::{
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicU64, Ordering},
    task::{Context, Poll, Waker},
};

pub struct JoinHandle<Output>(Option<Task<Output>>);

impl<Output> JoinHandle<Output> {
    pub fn cancel(mut self) {
        if let Some(task) = self.0.take() {
            drop(task);
        }
    }

    pub async fn stop(mut self) -> Option<Output> {
        if let Some(task) = self.0.take() {
            task.cancel().await
        } else {
            None
        }
    }
}

impl<O> Future for JoinHandle<O> {
    type Output = O;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0.as_mut().unwrap()).poll(cx)
    }
}

impl<T> Drop for JoinHandle<T> {
    fn drop(&mut self) {
        if let Some(task) = self.0.take() {
            task.detach();
        }
    }
}

type Queue = Arc<queue::span::Queue<queue::vec_deque::Queue<Runnable>>>;

fn new_queue() -> Queue {
    let queue = queue::vec_deque::Queue::default();
    let queue = queue::span::Queue::new(queue, "bach::executor");
    Arc::new(queue)
}

pub struct Executor<E: Environment> {
    environment: E,
    queue: Queue,
    handle: Handle,
}

impl<E: Environment> Executor<E> {
    pub fn new<F: FnOnce(&Handle) -> E>(create_env: F) -> Self {
        let queue = new_queue();

        let handle = Handle {
            sender: queue.clone(),
            primary_count: Default::default(),
            ids: Default::default(),
        };

        let environment = create_env(&handle);

        Self {
            environment,
            queue,
            handle,
        }
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

    pub fn microstep(&mut self) -> Poll<usize> {
        let task_count = self.queue.len();

        if task_count == 0 {
            return Poll::Ready(0);
        }

        // make the drain lazy so the environment can be entered
        let tasks = Some(&self.queue)
            .into_iter()
            .flat_map(|v| v.drain())
            .map(|runnable| {
                move || {
                    if runnable.run() {
                        Poll::Pending
                    } else {
                        Poll::Ready(())
                    }
                }
            });

        if self.environment.run(tasks).is_ready() {
            Poll::Ready(task_count)
        } else {
            Poll::Pending
        }
    }

    pub fn macrostep(&mut self) -> Macrostep {
        loop {
            if let Poll::Ready(tasks) = self.microstep() {
                let macrostep = Macrostep { tasks, ticks: 0 };
                let macrostep = self.environment.on_macrostep(macrostep);

                self.environment.enter(|| macrostep.metrics());

                return macrostep;
            }
        }
    }

    pub fn block_on<T, Output>(&mut self, task: T) -> Output
    where
        T: 'static + Future<Output = Output> + Send,
        Output: 'static + Send,
    {
        use core::task::{RawWaker, RawWakerVTable};

        const VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
        unsafe fn clone(ptr: *const ()) -> RawWaker {
            RawWaker::new(ptr, &VTABLE)
        }
        unsafe fn noop(_ptr: *const ()) {
            // noop
        }

        let mut task = self.spawn(task);
        let waker = unsafe { Waker::from_raw(clone(core::ptr::null())) };
        let mut ctx = Context::from_waker(&waker);

        loop {
            self.macrostep();

            if let Poll::Ready(value) = Pin::new(&mut task).poll(&mut ctx) {
                return value;
            }
        }
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
        // drop the pending items in the queue first
        let queue = self.queue.clone();
        self.environment.close(move || {
            let _ = queue.close();
            drop(queue.drain());
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
    sender: Queue,
    primary_count: Arc<AtomicU64>,
    ids: Arc<AtomicU64>,
}

impl Handle {
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

        let sender = self.sender.clone();

        let id = self.ids.fetch_add(1, Ordering::Relaxed);
        let name = Arc::from(name.to_string());

        let future = crate::task::info::WithInfo::new(future, id, &name);

        let (runnable, task) = async_task::spawn(future, move |runnable| {
            if name.is_empty() {
                count!("wake", "target" = id.to_string());
            } else {
                count!("wake", "target" = name.clone());
            }
            let _ = sender.push(runnable);
        });

        // queue the initial poll
        runnable.schedule();

        JoinHandle(Some(task))
    }

    pub fn enter<F: FnOnce() -> O, O>(&self, f: F) -> O {
        crate::task::scope::with(self.clone(), f)
    }

    pub fn primary_guard(&self) -> crate::task::primary::Guard {
        crate::task::primary::Guard::new(self.primary_count.clone())
    }

    fn primary_count(&self) -> u64 {
        self.primary_count.load(Ordering::SeqCst)
    }
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
        fn enter<F: FnOnce() -> O, O>(&self, f: F) -> O {
            f()
        }

        fn run<Tasks, F>(&mut self, tasks: Tasks) -> Poll<()>
        where
            Tasks: IntoIterator<Item = F>,
            F: 'static + FnOnce() -> Poll<()> + Send,
        {
            let mut is_ready = true;
            for task in tasks {
                is_ready &= task().is_ready();
            }
            if is_ready {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
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

    #[test]
    fn basic_test() {
        crate::testing::init_tracing();

        let mut executor = executor();

        let queue = Arc::new(queue::vec_deque::Queue::default());

        crate::task::scope::with(executor.handle().clone(), || {
            use crate::task::spawn;

            let s1 = queue.clone();
            spawn(async move {
                Yield::default().await;
                let _ = s1.push("hello");
                Yield::default().await;
            });

            let s2 = queue.clone();
            let exclaimation = async move {
                Yield::default().await;
                let _ = s2.push("!!!!!");
                Yield::default().await;
            };

            let s3 = queue.clone();
            spawn(async move {
                Yield::default().await;
                let _ = s3.push("world");
                Yield::default().await;
                exclaimation.await;
                Yield::default().await;
            });
        });

        executor.macrostep();

        let mut output = String::new();
        for chunk in queue.drain() {
            output.push_str(chunk);
        }

        assert_eq!(output, "helloworld!!!!!");
    }
}
