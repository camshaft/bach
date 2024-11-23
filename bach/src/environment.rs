use core::task::Poll;

pub mod default;
mod macrostep;
pub use macrostep::Macrostep;

pub trait Environment {
    fn enter<F: FnOnce() -> O, O>(&mut self, f: F) -> O;

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
        struct Close<F>(F);

        impl<F> Runnable for Close<F>
        where
            F: 'static + FnOnce() + Send,
        {
            fn run(self) -> Poll<()> {
                (self.0)();
                Poll::Ready(())
            }
        }

        let _ = self.run(Some(Close(close)));
    }
}

pub trait Runnable: 'static + Send {
    fn run(self) -> Poll<()>;
}

impl Runnable for async_task::Runnable {
    fn run(self) -> Poll<()> {
        if self.run() {
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}
