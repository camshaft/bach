use crate::{executor, rand, time::scheduler};
use core::task::Poll;

pub struct Runtime {
    inner: executor::Executor<Environment>,
}

impl Default for Runtime {
    fn default() -> Self {
        let inner = executor::Executor::new(|handle| Environment {
            handle: handle.clone(),
            time: scheduler::Scheduler::new(),
            rand: rand::Scope::new(0),
            stalled_iterations: 0,
        });

        Self { inner }
    }
}

impl Runtime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_seed(mut self, seed: u64) -> Self {
        self.inner.environment().rand = rand::Scope::new(seed);
        self
    }

    pub fn run<F: FnOnce() -> R, R>(&mut self, f: F) -> R {
        let result = self.inner.environment().enter(f);

        self.inner.block_on_primary();

        result
    }

    pub fn block_on<F>(&mut self, f: F) -> F::Output
    where
        F: 'static + Send + core::future::Future,
        F::Output: Send,
    {
        self.inner.block_on(f)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.inner.close();
    }
}

pub struct Environment {
    handle: executor::Handle,
    time: scheduler::Scheduler,
    rand: rand::Scope,
    stalled_iterations: usize,
    // TODO network
}

impl Environment {
    fn enter<F: FnOnce() -> O, O>(&self, f: F) -> O {
        self.handle.enter(|| self.time.enter(|| self.rand.enter(f)))
    }

    fn close<F: FnOnce()>(&mut self, f: F) {
        let handle = &mut self.handle;
        let rand = &mut self.rand;
        let time = &mut self.time;
        handle.enter(|| {
            rand.enter(|| {
                time.close();
                time.enter(|| {
                    f();
                });
            })
        })
    }
}

impl super::Environment for Environment {
    fn run<Tasks, F>(&mut self, tasks: Tasks) -> Poll<()>
    where
        Tasks: Iterator<Item = F> + Send,
        F: 'static + FnOnce() -> Poll<()> + Send,
    {
        let mut is_ready = true;

        let Self {
            handle, time, rand, ..
        } = self;

        handle.enter(|| {
            time.enter(|| {
                rand.enter(|| {
                    // TODO run network here

                    for task in tasks {
                        is_ready &= task().is_ready();
                    }
                })
            })
        });

        if is_ready {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }

    fn on_macrostep(&mut self, count: usize) {
        // only advance time after a stall
        if count > 0 {
            self.stalled_iterations = 0;
            return;
        }

        self.stalled_iterations += 1;

        // A stalled iteration is a macrostep that didn't actually execute any tasks.
        //
        // The idea with limiting it prevents the runtime from looping endlessly and not
        // actually doing any work. The value of 100 was chosen somewhat arbitrarily as a high
        // enough number that we won't get false positives but low enough that the number of
        // loops stays within reasonable ranges.
        if self.stalled_iterations > 100 {
            panic!("the runtime stalled after 100 iterations");
        }

        while let Some(time) = self.time.advance() {
            let _ = time;
            if self.time.wake() > 0 {
                // if a task has woken, then reset the stall count
                self.stalled_iterations = 0;
                break;
            }
        }
    }

    fn close<F>(&mut self, close: F)
    where
        F: 'static + FnOnce() + Send,
    {
        Self::close(self, close)
    }
}
