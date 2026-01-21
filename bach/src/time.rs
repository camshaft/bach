use core::{fmt, ops};

mod bitset;
mod entry;
pub mod error;
pub mod scheduler;
mod stack;
mod timeout;
mod wheel;

pub use core::time::Duration;
pub use scheduler::Timer as Sleep;
pub use timeout::*;

pub fn sleep(duration: Duration) -> Sleep {
    measure!("sleep", duration);
    scheduler::scope::borrow_with(|handle| {
        let ticks = resolution::duration_to_ticks(duration);
        handle.delay(ticks)
    })
}

pub use self::sleep as delay;

pub fn sleep_until(target: Instant) -> Sleep {
    let now = Instant::now();
    let duration = target.0.saturating_sub(now.0);
    sleep(duration)
}

#[derive(Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
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

    pub fn try_elapsed(self) -> Option<Duration> {
        let now = Self::try_now()?;
        Some(now.0 - self.0)
    }

    pub fn elapsed_since_start(self) -> Duration {
        self.0
    }

    pub fn has_elapsed(&self) -> bool {
        Self::now().ge(self)
    }

    pub fn saturating_duration_since(self, earlier: Instant) -> Duration {
        self.0.saturating_sub(earlier.0)
    }

    pub(crate) fn from_ticks(ticks: u64) -> Self {
        let duration = resolution::ticks_to_duration(ticks);
        Self(duration)
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn zero() -> Self {
        Self(Duration::ZERO)
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

impl fmt::Debug for Instant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Instant")
            .field(&format_args!("{self:#}"))
            .finish()
    }
}

impl fmt::Display for Instant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let duration = self.elapsed_since_start();
        let nanos = duration.subsec_nanos();
        let secs = duration.as_secs() % 60;
        let mins = duration.as_secs() / 60 % 60;
        let hours = duration.as_secs() / 60 / 60;
        if f.alternate() {
            let days = duration.as_secs() / 86400;
            match (days, hours, mins) {
                (0, 0, 0) => write!(f, "{secs}.{nanos:09}"),
                (0, 0, _) => write!(f, "{mins}:{secs:02}.{nanos:09}"),
                (0, _, _) => write!(f, "{hours}:{mins:02}:{secs:02}.{nanos:09}"),
                (_, _, _) => write!(f, "{days}:{hours}:{mins:02}:{secs:02}.{nanos:09}"),
            }
        } else {
            write!(f, "{hours}:{mins:02}:{secs:02}.{nanos:09}")
        }
    }
}

pub use resolution::{tick_duration, with_tick_duration};

pub(crate) mod resolution {
    use core::time::Duration;
    crate::scope::define!(scope, Duration);

    pub fn tick_duration() -> Duration {
        scope::try_borrow_with(|v| v.unwrap_or_else(|| Duration::from_nanos(1)))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instant_display_alternate_format() {
        // Test case from the issue: 1 second and 1000 nanos should not display as "1.1000"
        // which looks like 1.1 seconds, but should display with 9-digit nanosecond padding
        let instant = Instant(Duration::new(1, 1000));
        let display = format!("{instant:#}");
        assert_eq!(display, "1.000001000", "1 second + 1000 nanos should show as 1.000001000, not 1.1000");

        // Test 0.5 seconds (500 million nanos)
        let instant = Instant(Duration::new(0, 500_000_000));
        let display = format!("{instant:#}");
        assert_eq!(display, "0.500000000", "0.5 seconds should display correctly");

        // Test 1 second exactly
        let instant = Instant(Duration::new(1, 0));
        let display = format!("{instant:#}");
        assert_eq!(display, "1.000000000", "1 second should show with 9 zeros");

        // Test just nanoseconds
        let instant = Instant(Duration::new(0, 1000));
        let display = format!("{instant:#}");
        assert_eq!(display, "0.000001000", "1000 nanos should display with proper padding");

        // Test with minutes and seconds
        let instant = Instant(Duration::new(65, 123_456_789));
        let display = format!("{instant:#}");
        assert_eq!(display, "1:05.123456789", "65.123456789 seconds should format as minutes:seconds.nanos");

        // Test with hours
        let instant = Instant(Duration::new(3661, 999_999_999));
        let display = format!("{instant:#}");
        assert_eq!(display, "1:01:01.999999999", "3661.999999999 seconds should format as hours:minutes:seconds.nanos");

        // Test with days
        let instant = Instant(Duration::new(86400 + 3661, 1));
        let display = format!("{instant:#}");
        assert_eq!(display, "1:25:01:01.000000001", "1 day + 3661.000000001 seconds should include days");
    }

    #[test]
    fn instant_display_regular_format() {
        // Verify non-alternate format still works correctly
        let instant = Instant(Duration::new(1, 1000));
        let display = format!("{instant}");
        assert_eq!(display, "0:00:01.000001000", "Regular format should show hours:minutes:seconds.nanos");

        let instant = Instant(Duration::new(3661, 123_456_789));
        let display = format!("{instant}");
        assert_eq!(display, "1:01:01.123456789", "Regular format with hours should work");
    }
}
