use core::task::Poll;

pub mod default;
mod macrostep;
#[cfg(feature = "net")]
pub mod net;
pub use macrostep::Macrostep;

pub trait Environment {
    fn enter<F: FnOnce() -> O, O>(&mut self, f: F) -> O;

    /// This function is unused - preserving until 0.1 release
    fn run<Tasks, R>(&mut self, tasks: Tasks) -> Poll<()>
    where
        Tasks: IntoIterator<Item = R>,
        R: Runnable;

    fn on_macrostep(&mut self, macrostep: Macrostep) -> Macrostep {
        macrostep
    }

    fn close<F>(&mut self, close: F)
    where
        F: 'static + FnOnce() + Send,
    {
        self.enter(close);
    }
}

pub trait Runnable: 'static + Send {
    fn run(self) -> Poll<()>;
}
