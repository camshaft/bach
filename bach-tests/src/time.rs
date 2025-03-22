use crate::sim;
use bach::{
    ext::*,
    time::{sleep, Instant},
};
use bolero::{check, produce};
use std::time::Duration;

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
