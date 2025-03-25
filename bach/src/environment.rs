use core::task::Poll;

pub mod default;
mod macrostep;
#[cfg(feature = "net")]
pub mod net;
pub use macrostep::Macrostep;

pub trait Environment {
    fn enter<F: FnOnce(u64) -> O, O>(&mut self, f: F) -> O;

    fn on_microsteps<F: FnMut(u64) -> usize>(&mut self, mut f: F) {
        self.enter(|current_ticks| while f(current_ticks) > 0 {})
    }

    fn on_macrostep(&mut self, macrostep: Macrostep) -> Macrostep {
        macrostep
    }

    fn close<F>(&mut self, close: F)
    where
        F: 'static + FnOnce() + Send,
    {
        self.enter(|_| close());
    }
}

pub trait Runnable: 'static + Send {
    fn run(self) -> Poll<()>;
}
