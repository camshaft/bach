use alloc::sync::Arc;
use parking_lot::Mutex;
use rand::{distributions, prelude::*};

crate::scope::define!(scope, Scope);

pub fn gen<T>() -> T
where
    distributions::Standard: Distribution<T>,
{
    scope::borrow_mut_with(|scope| scope.gen())
}

pub fn gen_range<B, T>(range: B) -> T
where
    B: distributions::uniform::SampleRange<T>,
    T: distributions::uniform::SampleUniform + PartialOrd,
{
    scope::borrow_mut_with(|scope| scope.gen_range(range))
}

pub fn shuffle<T>(items: &mut [T]) {
    scope::borrow_mut_with(|scope| items.shuffle(scope))
}

pub fn swap<T>(items: &mut [T]) {
    swap_count(items, 1)
}

pub fn swap_count<T>(items: &mut [T], count: usize) {
    scope::borrow_mut_with(|r| {
        let mut r = r.rng.lock();
        for _ in 0..count {
            let a = r.gen_range(0..items.len());
            let b = r.gen_range(0..items.len());
            items.swap(a, b)
        }
    })
}

pub fn one_of<T>(items: &[T]) -> &T {
    let index = gen_range(0..items.len());
    &items[index]
}

#[derive(Clone)]
pub struct Scope {
    rng: Arc<Mutex<SmallRng>>,
}

impl Scope {
    pub fn new(seed: u64) -> Self {
        let rng = SmallRng::seed_from_u64(seed);
        let rng = Arc::new(Mutex::new(rng));
        Self { rng }
    }

    pub fn enter<F: FnOnce() -> O, O>(&self, f: F) -> O {
        scope::with(self.clone(), f)
    }
}

impl RngCore for Scope {
    fn next_u32(&mut self) -> u32 {
        self.rng.lock().next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.rng.lock().next_u64()
    }

    fn fill_bytes(&mut self, bytes: &mut [u8]) {
        self.rng.lock().fill_bytes(bytes)
    }

    fn try_fill_bytes(&mut self, bytes: &mut [u8]) -> Result<(), rand::Error> {
        self.rng.lock().try_fill_bytes(bytes)
    }
}
