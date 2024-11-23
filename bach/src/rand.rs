use alloc::sync::Arc;
use bolero_generator::{
    driver::{self, object::DynDriver, Driver},
    TypeGenerator, ValueGenerator,
};
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use pin_project_lite::pin_project;
use rand::prelude::*;
use rand_xoshiro::Xoshiro256PlusPlus;
use std::sync::Mutex;

mod exhaustive;
pub use exhaustive::Exhaustive;

crate::scope::define!(scope, Scope);

#[inline]
pub fn fill_bytes(bytes: &mut [u8]) {
    driver(|driver| {
        let len = bytes.len();
        let _ = driver.gen_from_bytes(
            || (len, Some(len)),
            |src| {
                bytes.copy_from_slice(src);
                Some((src.len(), ()))
            },
        );
    })
}

#[inline]
pub fn fill<T>(values: &mut [T])
where
    T: TypeGenerator,
{
    driver(|driver| {
        for value in values {
            *value = T::generate(driver).unwrap();
        }
    })
}

#[inline]
pub fn gen<T>() -> T
where
    T: TypeGenerator,
{
    driver(|driver| T::generate(driver).unwrap())
}

#[inline]
pub fn gen_range<R>(range: R) -> R::Output
where
    R: ValueGenerator,
{
    driver(|driver| range.generate(driver).unwrap())
}

#[inline]
pub fn shuffle<T>(items: &mut [T]) {
    driver(|driver| {
        for i in (1..items.len()).rev() {
            let idx = (0..=i).generate(driver).unwrap();
            // invariant: elements with index > i have been locked in place.
            items.swap(i, idx);
        }
    });
}

#[inline]
pub fn swap<T>(items: &mut [T]) {
    swap_count(items, 1)
}

#[inline]
pub fn swap_count<T>(items: &mut [T], count: usize) {
    driver(|driver| {
        for _ in 0..count {
            let a = (0..items.len()).generate(driver).unwrap();
            let b = (0..items.len()).generate(driver).unwrap();
            items.swap(a, b)
        }
    })
}

#[inline]
pub fn one_of<T>(items: &[T]) -> &T {
    let index = gen_range(0..items.len());
    &items[index]
}

fn driver<F: FnOnce(&mut driver::object::Borrowed) -> R, R>(f: F) -> R {
    scope::borrow_mut_with(|scope| scope.with(f))
}

#[derive(Clone)]
pub struct Scope {
    driver: Arc<Mutex<dyn DynDriver>>,
    // if this scope allows for child scopes
    can_have_children: bool,
}

impl Scope {
    pub fn new(seed: u64) -> Self {
        let rng = Xoshiro256PlusPlus::seed_from_u64(seed);
        let driver = driver::Rng::new(rng, &Default::default());
        let driver = driver::object::Object(driver);
        let driver = Arc::new(Mutex::new(driver));
        Self {
            driver,
            can_have_children: true,
        }
    }

    pub fn enter<F: FnOnce() -> O, O>(&self, f: F) -> O {
        let (prev, can_have_children) = scope::try_borrow_mut_with(|r| {
            let can_have_children = r.as_ref().map_or(true, |r| r.can_have_children);
            if !can_have_children {
                return (None, false);
            }
            let prev = core::mem::replace(r, Some(self.clone()));
            (prev, can_have_children)
        });

        if !can_have_children {
            return f();
        }

        let out = f();
        scope::set(prev);
        out
    }

    fn with<F: FnOnce(&mut driver::object::Borrowed) -> R, R>(&self, f: F) -> R {
        let mut driver = self.driver.lock().unwrap();
        let mut driver = driver::object::Borrowed(&mut *driver);
        f(&mut driver)
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
