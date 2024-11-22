use core::task::Poll;

pub mod default;

mod macrostep {
    #[derive(Clone, Copy, Debug, Default)]
    pub struct Macrostep {
        pub tasks: usize,
        pub ticks: u64,
    }

    impl Macrostep {
        pub fn metrics(&self) {
            measure!("tasks", self.tasks as u32);
            measure!(
                "advance",
                crate::time::resolution::ticks_to_duration(self.ticks)
            );
        }
    }
}

pub use macrostep::Macrostep;

pub trait Environment {
    fn enter<F: FnOnce() -> O, O>(&self, f: F) -> O;

    fn run<Tasks, F>(&mut self, tasks: Tasks) -> Poll<()>
    where
        Tasks: IntoIterator<Item = F>,
        F: 'static + FnOnce() -> Poll<()> + Send;

    fn on_macrostep(&mut self, macrostep: Macrostep) -> Macrostep {
        macrostep
    }

    fn close<F>(&mut self, close: F)
    where
        F: 'static + FnOnce() + Send,
    {
        let _ = self.run(Some(move || {
            close();
            Poll::Ready(())
        }));
    }
}
