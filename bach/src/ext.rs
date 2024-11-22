use core::time::Duration;

pub use crate::{
    group::GroupExt,
    sync::queue::{InstantQueueExt, QueueExt},
};

pub trait DurationLiteral {
    fn s(self) -> Duration;
    fn ms(self) -> Duration;
    fn us(self) -> Duration;
    fn ns(self) -> Duration;
}

impl DurationLiteral for u64 {
    fn s(self) -> Duration {
        Duration::from_secs(self)
    }

    fn ms(self) -> Duration {
        Duration::from_millis(self)
    }

    fn us(self) -> Duration {
        Duration::from_micros(self)
    }

    fn ns(self) -> Duration {
        Duration::from_nanos(self)
    }
}

pub trait SleepExt {
    type Output;

    fn sleep(self) -> Self::Output;
}

impl SleepExt for Duration {
    type Output = crate::time::scheduler::Timer;

    fn sleep(self) -> Self::Output {
        crate::time::sleep(self)
    }
}

pub trait SpawnExt {
    type Output;

    fn spawn(self) -> Self::Output;
    fn spawn_named<N: core::fmt::Display>(self, name: N) -> Self::Output;
}

impl<F> SpawnExt for F
where
    F: 'static + Send + core::future::Future,
    F::Output: Send,
{
    type Output = crate::executor::JoinHandle<F::Output>;

    fn spawn(self) -> Self::Output {
        crate::task::spawn(self)
    }

    fn spawn_named<N: core::fmt::Display>(self, name: N) -> Self::Output {
        crate::task::spawn_named(self, name)
    }
}

pub trait PrimaryExt {
    type Output;

    fn primary(self) -> Self::Output;
}

impl<F> PrimaryExt for F
where
    F: core::future::Future,
{
    type Output = crate::task::primary::Wrapped<F>;

    fn primary(self) -> Self::Output {
        crate::task::primary::create(self)
    }
}

pub trait SeedExt {
    type Output;

    fn with_seed<S: Into<crate::rand::Scope>>(self, seed: S) -> Self::Output;
}

impl<F> SeedExt for F
where
    F: core::future::Future,
{
    type Output = crate::rand::Task<F>;

    fn with_seed<S: Into<crate::rand::Scope>>(self, seed: S) -> Self::Output {
        crate::rand::Task::new(self, seed.into())
    }
}
