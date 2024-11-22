use core::fmt;

mod bitset;
mod entry;
pub mod scheduler;
mod stack;
mod wheel;

pub use core::time::Duration;

pub fn sleep(duration: Duration) -> scheduler::Timer {
    measure!("sleep", duration);
    scheduler::scope::borrow_with(|handle| {
        let ticks = resolution::duration_to_ticks(duration);
        handle.delay(ticks)
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

impl fmt::Display for Instant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let duration = self.elapsed_since_start();
        let nanos = duration.subsec_nanos();
        let secs = duration.as_secs() % 60;
        let mins = duration.as_secs() / 60 % 60;
        let hours = duration.as_secs() / 60 / 60;
        write!(f, "{hours}:{mins:02}:{secs:02}.{nanos:09}")
    }
}

fn now() -> Instant {
    scheduler::scope::borrow_with(|handle| handle.now())
}

pub use resolution::{tick_duration, with_tick_duration};

pub(crate) mod resolution {
    use core::time::Duration;
    crate::scope::define!(scope, Duration);

    pub fn tick_duration() -> Duration {
        scope::try_borrow_with(|v| v.unwrap_or_else(|| Duration::from_micros(1)))
    }

    pub use scope::with as with_tick_duration;

    pub fn ticks_to_duration(ticks: u64) -> Duration {
        let nanos_per_tick = tick_duration().as_nanos() as u64;

        let nanos = nanos_per_tick * ticks;
        Duration::from_nanos(nanos)
    }

    pub fn duration_to_ticks(duration: Duration) -> u64 {
        let nanos = duration.as_nanos();
        let nanos_per_tick = tick_duration().as_nanos();
        let ticks = nanos / nanos_per_tick;
        ticks as u64
    }
}
