use core::task::Poll;

pub mod default;

pub trait Environment {
    fn run<Tasks, F>(&mut self, tasks: Tasks) -> Poll<()>
    where
        Tasks: Iterator<Item = F> + Send,
        F: 'static + FnOnce() -> Poll<()> + Send;

    fn on_macrostep(&mut self, count: usize) {
        let _ = count;
    }

    fn close<F>(&mut self, close: F)
    where
        F: 'static + FnOnce() + Send,
    {
        let _ = self.run(
            Some(move || {
                close();
                Poll::Ready(())
            })
            .into_iter(),
        );
    }
}
