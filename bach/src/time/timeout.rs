use crate::time::{error::Elapsed, scheduler::Timer, sleep_until, Duration, Instant};
use pin_project_lite::pin_project;
use std::{
    future::{Future, IntoFuture},
    pin::Pin,
    task::{self, Poll},
};

#[track_caller]
pub fn timeout<F>(duration: Duration, future: F) -> Timeout<F::IntoFuture>
where
    F: IntoFuture,
{
    let delay = super::sleep(duration);
    Timeout::new_with_delay(future.into_future(), delay)
}

pub fn timeout_at<F>(deadline: Instant, future: F) -> Timeout<F::IntoFuture>
where
    F: IntoFuture,
{
    let delay = sleep_until(deadline);

    Timeout {
        value: future.into_future(),
        delay,
    }
}

pin_project! {
    /// Future returned by [`timeout`](timeout) and [`timeout_at`](timeout_at).
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    #[derive(Debug)]
    pub struct Timeout<T> {
        #[pin]
        value: T,
        #[pin]
        delay: Timer,
    }
}

impl<T> Timeout<T> {
    pub(crate) fn new_with_delay(value: T, delay: Timer) -> Timeout<T> {
        Timeout { value, delay }
    }

    /// Gets a reference to the underlying value in this timeout.
    pub fn get_ref(&self) -> &T {
        &self.value
    }

    /// Gets a mutable reference to the underlying value in this timeout.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Consumes this timeout, returning the underlying value.
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T> Future for Timeout<T>
where
    T: Future,
{
    type Output = Result<T::Output, Elapsed>;

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        let me = self.project();

        // First, try polling the future
        if let Poll::Ready(v) = me.value.poll(cx) {
            return Poll::Ready(Ok(v));
        }

        let delay = me.delay;

        match delay.poll(cx) {
            Poll::Ready(()) => Poll::Ready(Err(Elapsed::new())),
            Poll::Pending => Poll::Pending,
        }
    }
}
