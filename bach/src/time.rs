use core::time::Duration;

mod bitset;
mod entry;
pub mod scheduler;
mod stack;
mod wheel;

pub fn sleep(duration: Duration) -> scheduler::Timer {
    scheduler::scope::borrow_with(|handle| {
        let nanos = duration.as_nanos();
        let nanos_per_tick = resolution::tick_duration().as_nanos();
        let ticks = nanos / nanos_per_tick;

        handle.delay(ticks as u64)
    })
}

pub use self::sleep as delay;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct Instant(Duration);

impl Instant {
    pub fn now() -> Self {
        now()
    }

    pub fn elapsed(self) -> Duration {
        Self::now().0 - self.0
    }

    pub fn elapsed_since_start(self) -> Duration {
        self.0
    }
}

fn now() -> Instant {
    scheduler::scope::borrow_with(|handle| {
        let nanos_per_tick = resolution::tick_duration().as_nanos() as u64;

        let ticks = handle.ticks();
        let nanos = nanos_per_tick * ticks;
        Instant(Duration::from_nanos(nanos))
    })
}

pub use resolution::{tick_duration, with_tick_duration};

mod resolution {
    use core::time::Duration;
    crate::scope::define!(scope, Duration);

    pub fn tick_duration() -> Duration {
        scope::try_borrow_with(|v| v.unwrap_or_else(|| Duration::from_micros(1)))
    }

    pub use scope::with as with_tick_duration;
}
