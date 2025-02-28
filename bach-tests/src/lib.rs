#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(test)]
mod coop;
#[cfg(test)]
mod panics;
#[cfg(test)]
mod queue;
#[cfg(test)]
mod testing;
