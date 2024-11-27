use core::{fmt, ops};

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

pub fn sleep_until(target: Instant) -> scheduler::Timer {
    let now = Instant::now();
    let duration = target.0.saturating_sub(now.0);
    sleep(duration)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct Instant(Duration);

impl Instant {
    pub fn now() -> Self {
        scheduler::scope::borrow_with(|v| v.now())
    }

    pub fn try_now() -> Option<Self> {
        scheduler::scope::try_borrow_with(|v| v.as_ref().map(|v| v.now()))
    }

    pub fn elapsed(self) -> Duration {
        Self::now().0 - self.0
    }

    pub fn elapsed_since_start(self) -> Duration {
        self.0
    }

    pub fn has_elapsed(&self) -> bool {
        Self::now().ge(self)
    }
}

impl ops::Add<Duration> for Instant {
    type Output = Instant;

    fn add(self, rhs: Duration) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl ops::AddAssign<Duration> for Instant {
    fn add_assign(&mut self, rhs: Duration) {
        self.0 += rhs;
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
