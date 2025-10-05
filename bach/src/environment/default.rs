use super::Macrostep;
use crate::{
    coop::Coop,
    environment::{Environment as _, Runner},
    executor, rand,
    task::supervisor::RunOutcome,
    time::scheduler,
};
use std::time::Duration;

#[cfg(feature = "net")]
use crate::environment::net;

pub struct Runtime<R: Runner = DefaultRunner> {
    inner: executor::Executor<Environment<R>>,
}

impl<R: Runner + Default> Default for Runtime<R> {
    fn default() -> Self {
        Self::new_with_runner(Default::default())
    }
}

impl Runtime<DefaultRunner> {
    pub fn new() -> Self {
        Self::new_with_runner(DefaultRunner::default())
    }
}

impl<R: Runner> Runtime<R> {
    pub fn new_with_runner(runner: R) -> Self {
        let inner = executor::Executor::new(|handle| Environment {
            handle: handle.clone(),
            time: scheduler::Scheduler::new(),
            rand: None,
            coop: Coop::default(),
            stalled_iterations: 0,
            coop_enabled: false,
            #[cfg(feature = "net")]
            net: Some(Default::default()),
            runner,
        });

        Self { inner }
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

    pub fn run<F: FnOnce() -> Res, Res>(&mut self, f: F) -> Res {
        let result = self.inner.environment().enter(|_, _| f());

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
            .enter(|ticks| crate::time::Instant::from_ticks(ticks).elapsed_since_start())
    }
}

#[cfg(feature = "net")]
impl<R: Runner> Runtime<R> {
    pub fn with_net_queues(mut self, net: Option<Box<dyn net::queue::Allocator>>) -> Self {
        if let Some(queue) = net {
            let net = &mut self.inner.environment().net;
            if let Some(net) = net.as_mut() {
                net.set_queue(queue);
            } else {
                *net = Some(Box::new(net::registry::Registry::new(queue)));
            };
        } else {
            self.inner.environment().net = None;
        }
        self
    }

    pub fn with_subnet(mut self, subnet: crate::net::IpAddr) -> Self {
        if let Some(net) = self.inner.environment().net.as_mut() {
            net.set_subnet(subnet);
        }
        self
    }

    pub fn with_pcap_dir<P: Into<std::path::PathBuf>>(mut self, dir: P) -> std::io::Result<Self> {
        if let Some(net) = self.inner.environment().net.as_mut() {
            net.set_pcap_dir(dir)?;
        }
        Ok(self)
    }

    pub fn with_net_monitor(mut self, enabled: bool) -> Self {
        if let Some(net) = self.inner.environment().net.as_mut() {
            net.set_monitor(enabled);
        }
        self
    }
}

impl<R: Runner> Drop for Runtime<R> {
    fn drop(&mut self) {
        self.inner.close();
    }
}

pub struct Environment<Runner> {
    handle: executor::Handle,
    time: scheduler::Scheduler,
    rand: Option<rand::Scope>,
    stalled_iterations: usize,
    coop: Coop,
    coop_enabled: bool,
    #[cfg(feature = "net")]
    net: Option<Box<net::registry::Registry>>,
    runner: Runner,
}

impl<R: Runner> Environment<R> {
    fn close<F: FnOnce()>(&mut self, f: F) {
        let f = {
            #[cfg(not(feature = "coop"))]
            {
                f
            }

            #[cfg(feature = "coop")]
            {
                let enabled = self.coop_enabled;
                let coop = &mut self.coop;
                move || {
                    if enabled {
                        coop.enter(f)
                    } else {
                        f()
                    }
                }
            }
        };

        let f = {
            #[cfg(not(feature = "net"))]
            {
                f
            }

            #[cfg(feature = "net")]
            {
                let net = &mut self.net;
                move || {
                    if let Some(v) = net.take() {
                        // TODO close registry
                        let (v, res) = net::registry::scope::with(v, f);
                        drop(v);
                        res
                    } else {
                        f()
                    }
                }
            }
        };

        let rand = self.rand.as_mut();
        let f = move || {
            if let Some(rand) = rand {
                rand.enter(f)
            } else {
                f()
            }
        };

        self.handle.enter(|| {
            // Don't close the time scheduler - that will wake all of its tasks which we don't want.
            // Some of them may be monitoring for a timeout.
            // ```
            // async {
            //    sleep(Duration::from_secs(10)).await;
            //    panic!("simulation time exceede 10s");
            // }.spawn();
            // ```
            self.time.enter(|_| f())
        })
    }
}

impl<R: Runner> super::Environment for Environment<R> {
    type Runner = R;

    fn enter<'a, F: FnOnce(u64, &'a Self::Runner) -> O, O>(&'a mut self, f: F) -> O {
        let f = {
            #[cfg(not(feature = "coop"))]
            {
                f
            }

            #[cfg(feature = "coop")]
            {
                let enabled = self.coop_enabled;
                let coop = &mut self.coop;
                move |ticks, runner| {
                    if enabled {
                        coop.enter(|| f(ticks, runner))
                    } else {
                        f(ticks, runner)
                    }
                }
            }
        };

        let f = {
            #[cfg(not(feature = "net"))]
            {
                f
            }

            #[cfg(feature = "net")]
            {
                let net = &mut self.net;
                move |ticks, runner| {
                    if let Some(v) = net.take() {
                        let (v, res) = net::registry::scope::with(v, || f(ticks, runner));
                        *net = Some(v);
                        res
                    } else {
                        f(ticks, runner)
                    }
                }
            }
        };

        let rand = self.rand.as_mut();
        let f = move |ticks, runner| {
            if let Some(rand) = rand {
                rand.enter(|| f(ticks, runner))
            } else {
                f(ticks, runner)
            }
        };

        let runner = &self.runner;
        let f = move |ticks| f(ticks, runner);

        self.handle.enter(|| self.time.enter(f))
    }

    fn on_macrostep(&mut self, mut macrostep: Macrostep) -> Macrostep {
        // only advance time after a stall
        if macrostep.tasks > 0 {
            self.stalled_iterations = 0;
            return macrostep;
        }

        if cfg!(feature = "coop") && self.coop_enabled {
            let coop = &mut self.coop;
            let f = || coop.schedule();

            let mut f = || {
                if let Some(rand) = self.rand.as_mut() {
                    rand.enter(f)
                } else {
                    f()
                }
            };

            let tasks = self.handle.enter(|| self.time.enter(|_ticks| f()));

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
            macrostep.stalled = true;
            return macrostep;
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

#[derive(Clone, Copy, Default)]
pub struct DefaultRunner(());

impl Runner for DefaultRunner {
    fn run(
        &self,
        f: &mut crate::task::supervisor::DynRunnable,
        cx: &mut core::task::Context<'_>,
    ) -> RunOutcome {
        f.as_mut().poll(cx)
    }
}
