use crate::{coop::Coop, environment::Environment as _, executor, rand, time::scheduler};
use core::task::Poll;
use std::time::Duration;

use super::{Macrostep, Runnable};

pub struct Runtime {
    inner: executor::Executor<Environment>,
}

impl Default for Runtime {
    fn default() -> Self {
        let inner = executor::Executor::new(|handle| Environment {
            handle: handle.clone(),
            time: scheduler::Scheduler::new(),
            rand: Some(rand::Scope::new(0)),
            coop: Coop::default(),
            stalled_iterations: 0,
            coop_enabled: false,
        });

        Self { inner }
    }
}

impl Runtime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_seed(self, seed: u64) -> Self {
        self.with_rand(Some(rand::Scope::new(seed)))
    }

    pub fn with_rand(mut self, rand: Option<rand::Scope>) -> Self {
        self.inner.environment().rand = rand;
        self
    }

    pub fn with_coop(mut self, enabled: bool) -> Self {
        self.inner.environment().coop_enabled = enabled;
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

    pub fn elapsed(&mut self) -> Duration {
        self.inner
            .environment()
            .time
            .enter(|| crate::time::Instant::now().elapsed_since_start())
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
    rand: Option<rand::Scope>,
    stalled_iterations: usize,
    coop: Coop,
    coop_enabled: bool,
    // TODO network
}

impl Environment {
    fn close<F: FnOnce()>(&mut self, f: F) {
        let handle = &mut self.handle;
        let time = &mut self.time;
        handle.enter(|| {
            let e = || {
                time.close();
                time.enter(|| {
                    f();
                });
            };

            if let Some(rand) = self.rand.as_mut() {
                rand.enter(e)
            } else {
                e()
            }
        })
    }
}

impl super::Environment for Environment {
    fn enter<F: FnOnce() -> O, O>(&mut self, f: F) -> O {
        self.handle.enter(|| {
            self.time.enter(|| {
                let e = || {
                    if cfg!(feature = "coop") && self.coop_enabled {
                        self.coop.enter(f)
                    } else {
                        f()
                    }
                };

                if let Some(rand) = self.rand.as_mut() {
                    rand.enter(e)
                } else {
                    e()
                }
            })
        })
    }

    fn run<Tasks, R>(&mut self, tasks: Tasks) -> Poll<()>
    where
        Tasks: IntoIterator<Item = R>,
        R: Runnable,
    {
        let mut is_ready = true;

        self.enter(|| {
            for task in tasks {
                is_ready &= task.run().is_ready();
            }
        });

        if is_ready {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }

    fn on_macrostep(&mut self, mut macrostep: Macrostep) -> Macrostep {
        // only advance time after a stall
        if macrostep.tasks > 0 {
            self.stalled_iterations = 0;
            return macrostep;
        }

        if cfg!(feature = "coop") && self.coop_enabled {
            let tasks = if let Some(rand) = self.rand.as_mut() {
                rand.enter(|| self.coop.schedule())
            } else {
                self.coop.schedule()
            };
            macrostep.tasks += tasks;

            if macrostep.tasks > 0 {
                self.stalled_iterations = 0;
                return macrostep;
            }
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

        while let Some(ticks) = self.time.advance() {
            macrostep.ticks += ticks;

            macrostep.tasks += self.time.wake();

            if macrostep.tasks == 0 {
                continue;
            }

            // if a task has woken, then reset the stall count
            self.stalled_iterations = 0;
            break;
        }

        macrostep
    }

    fn close<F>(&mut self, close: F)
    where
        F: 'static + FnOnce() + Send,
    {
        Self::close(self, close)
    }
}
