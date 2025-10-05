use crate::sim;
use bach::{
    ext::*,
    time::{sleep, Instant},
};
use bolero::{check, produce};
use std::{
    task::{Context, Poll},
    time::Duration,
};
use tracing::info;

#[test]
fn secondary_task() {
    sim(|| {
        async {
            sleep(Duration::from_secs(1)).await;
        }
        .primary()
        .spawn();

        async {
            sleep(Duration::from_secs(10)).await;
            panic!("secondary task should not wake");
        }
        .spawn();
    });
}

#[test]
fn timer_test() {
    let min_time = Duration::from_nanos(1);
    let max_time = Duration::from_secs(3600);

    let delay = min_time..max_time;
    let count = 0u8..3;
    let delays = produce::<Vec<_>>().with().values((count, delay));

    async fn task(count: usize, delay: Duration) {
        for _ in 0..count {
            // get the time before the delay
            let now = Instant::now();

            // await the delay
            sleep(delay).await;

            // get the time that has passed on the clock and make sure it matches the amount that
            // was delayed
            let actual = Instant::now();
            let expected = now + delay;
            assert_eq!(
                actual, expected,
                "actual: {:?}, expected: {:?}",
                actual, expected
            );
        }
    }

    check!().with_generator(delays).for_each(|delays| {
        sim(|| {
            for (count, delay) in delays {
                task(*count as _, *delay).primary().spawn();
            }
        });
    });
}

#[test]
fn long_delays() {
    sim(|| {
        let mut delay = Duration::from_secs(1);
        for i in [1, 60, 60, 24, 365, 25, 4] {
            delay *= i;
            info!(?delay);
            async move {
                delay.sleep().await;
                info!("done");
            }
            .primary()
            .spawn();
        }
    });
}

#[test]
fn self_wake_pacing() {
    sim(|| {
        async {
            let mut pacer = Pacer::default();

            for _ in 0..100 {
                info!("before pace");
                pacer.pace().await;
                info!("after pace");
                1.ms().sleep().await;
                info!("after sleep");
            }
        }
        .primary()
        .spawn();
    });
}

#[derive(Default)]
pub struct Pacer {
    transmissions_without_yield: u8,
    yield_window: Option<Instant>,
}

impl Pacer {
    pub async fn pace(&mut self) {
        core::future::poll_fn(|cx| self.poll_pacing(cx)).await
    }

    #[inline]
    pub fn poll_pacing(&mut self, cx: &mut Context) -> Poll<()> {
        info!(self.transmissions_without_yield, "pace");

        if self.transmissions_without_yield < 5 {
            info!("pass");
            self.transmissions_without_yield += 1;
            return Poll::Ready(());
        }

        // reset the counter
        self.transmissions_without_yield = 0;

        // record the time that we yielded
        let now = Instant::now();
        let prev_yield_window = core::mem::replace(
            &mut self.yield_window,
            Some(now + core::time::Duration::from_millis(1)),
        );

        // if the current time falls outside of the previous window then don't actually yield - the
        // application isn't sending at that rate
        if let Some(yield_window) = prev_yield_window {
            if now > yield_window {
                info!("underflow");
                return Poll::Ready(());
            }
        }

        info!("yield");
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

#[test]
fn cost_modeling() {
    for _ in 0..2 {
        sim(|| {
            async {
                let before = Instant::now();
                assert_eq!(before.elapsed_since_start(), 0.ms());
                bach::cost::record(1.ms());
                let after = Instant::now();
                assert_eq!(after.saturating_duration_since(before), 1.ms());

                let before = Instant::now();
                bach::cost::record(1.ms());
                1.ms().sleep().await;
                let after = Instant::now();
                assert_eq!(after.saturating_duration_since(before), 2.ms());
            }
            .primary()
            .spawn();
        })
    }
}

#[test]
fn multi_task_cost_modeling() {
    sim(|| {
        for group in 0..2 {
            let group = format!("group-{group}");

            async {
                let before = Instant::now();
                1.ms().sleep().await;
                let after = Instant::now();
                assert_eq!(
                    after.saturating_duration_since(before),
                    2.ms(),
                    "the cost of the first task should be increased by the second"
                );
            }
            .primary()
            .group(&group)
            .spawn();

            async {
                let before = Instant::now();
                bach::cost::record(1.ms());
                let after = Instant::now();
                assert_eq!(after.saturating_duration_since(before), 1.ms());

                let before = Instant::now();
                bach::cost::record(1.ms());
                1.ms().sleep().await;
                let after = Instant::now();
                assert_eq!(after.saturating_duration_since(before), 2.ms());
            }
            .primary()
            .group(&group)
            .spawn();
        }
    })
}

#[test]
fn cost_modeling_spawn_inherit() {
    sim(|| {
        async {
            bach::cost::record(1.ms());

            async {
                assert_eq!(Instant::now().elapsed_since_start(), 2.ms());
            }
            .spawn()
            .primary();

            // record additional cost post-spawn to show that the child pays additional costs
            bach::cost::record(1.ms());
        }
        .primary()
        .group("group")
        .spawn();
    })
}

#[test]
fn timed_runner() {
    let runner = bach::environment::TimedRunner::default();
    let mut rt = bach::environment::default::Runtime::new_with_runner(runner);
    rt.run(|| {
        async {
            let before = Instant::now();
            std::thread::sleep(101.ms());
            bach::task::yield_now().await;
            let after = Instant::now();
            assert!(after.saturating_duration_since(before) >= 100.ms());
        }
        .primary()
        .spawn()
    });
}
