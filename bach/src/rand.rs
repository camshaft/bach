use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use pin_project_lite::pin_project;

pub use bolero_generator::prelude::*;

#[cfg(not(kani))]
mod standard {
    use bolero_generator::driver;
    use rand_xoshiro::{rand_core::SeedableRng, Xoshiro256PlusPlus};

    pub struct Scope {
        driver: Option<Box<driver::object::Object<driver::Rng<Xoshiro256PlusPlus>>>>,
    }

    impl Scope {
        pub fn new(seed: u64) -> Self {
            let rng = Xoshiro256PlusPlus::seed_from_u64(seed);
            let driver = driver::Rng::new(rng, &Default::default());
            let driver = driver::object::Object(driver);
            let driver = Box::new(driver);
            Self {
                driver: Some(driver),
            }
        }

        pub fn enter<F: FnOnce() -> O, O>(&mut self, f: F) -> O {
            let Some(driver) = self.driver.take() else {
                // the task likely panicked so just execute the function without the random scope
                return f();
            };
            let (driver, res) = bolero_generator::any::scope::with(driver, f);
            self.driver = Some(driver);
            res
        }
    }
}

/// Ensure compatibility with kani
#[cfg(any(kani, test))]
mod kani_impl {
    #![cfg_attr(test, allow(dead_code))]

    pub struct Scope(());

    impl Scope {
        pub fn new(seed: u64) -> Self {
            let _ = seed;
            Self(())
        }

        pub fn enter<F: FnOnce() -> O, O>(&mut self, f: F) -> O {
            f()
        }
    }
}

#[cfg(kani)]
pub use kani_impl::Scope;
#[cfg(not(kani))]
pub use standard::Scope;

impl From<u64> for Scope {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

pin_project! {
    pub struct Task<F> {
        #[pin]
        inner: F,
        scope: Scope,
    }
}

impl<F> Task<F> {
    pub fn new(inner: F, scope: Scope) -> Self {
        Self { inner, scope }
    }
}

impl<F: Future> Future for Task<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = self.project();
        let inner = this.inner;
        this.scope.enter(move || Future::poll(inner, cx))
    }
}
