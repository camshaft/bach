#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(all(test, feature = "coop"))]
mod coop;
#[cfg(test)]
mod panics;
#[cfg(test)]
mod queue;
#[cfg(test)]
mod testing;
