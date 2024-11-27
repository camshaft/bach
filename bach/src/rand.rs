use bolero_generator::driver;
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use pin_project_lite::pin_project;
use rand::prelude::*;
use rand_xoshiro::Xoshiro256PlusPlus;

pub use bolero_generator::prelude::*;

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
        let driver = self.driver.take().unwrap();
        let (driver, res) = bolero_generator::any::scope::with(driver, f);
        self.driver = Some(driver);
        res
    }
}

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
