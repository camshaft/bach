use crate::task::supervisor::RunOutcome;
use core::task::Poll;

pub mod default;
mod macrostep;
#[cfg(feature = "net")]
pub mod net;
pub use macrostep::Macrostep;

pub trait Environment {
    type Runner: Runner;

    fn enter<'a, F: FnOnce(u64, &'a Self::Runner) -> O, O>(&'a mut self, f: F) -> O;

    fn on_microsteps<'a, F: FnMut(u64, &'a Self::Runner) -> usize>(&'a mut self, mut f: F) {
        self.enter(|current_ticks, runner| while f(current_ticks, runner) > 0 {})
    }

    fn on_macrostep(&mut self, macrostep: Macrostep) -> Macrostep {
        macrostep
    }

    fn close<F>(&mut self, close: F)
    where
        F: 'static + FnOnce() + Send,
    {
        self.enter(|_, _| close());
    }
}

pub trait Runnable: 'static + Send {
    fn run(self) -> Poll<()>;
}

pub trait Runner {
    fn run(
        &self,
        f: &mut crate::task::supervisor::DynRunnable,
        cx: &mut core::task::Context<'_>,
    ) -> RunOutcome;
}

/// Uses measured poll times of tasks to model execution costs
#[derive(Clone, Debug)]
pub struct TimedRunner {
    threshold: core::time::Duration,
}

impl Default for TimedRunner {
    fn default() -> Self {
        Self {
            threshold: core::time::Duration::from_millis(1),
        }
    }
}

impl TimedRunner {
    pub fn with_threshold(mut self, threshold: core::time::Duration) -> Self {
        self.threshold = threshold;
        self
    }
}

impl Runner for TimedRunner {
    fn run(
        &self,
        f: &mut crate::task::supervisor::DynRunnable,
        cx: &mut core::task::Context<'_>,
    ) -> RunOutcome {
        let before = std::time::Instant::now();
        match f.as_mut().poll(cx) {
            RunOutcome::ExecutedApplication => {
                let elapsed = before.elapsed();
                f.as_mut().record_cost(elapsed);
                RunOutcome::ExecutedApplication
            }
            RunOutcome::PayingDebt => {
                // don't record debt while it's being paid
                RunOutcome::PayingDebt
            }
            RunOutcome::Done(value) => RunOutcome::Done(value),
        }
    }
}
