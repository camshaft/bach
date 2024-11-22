use super::{CloseError, PopError, PushError};
use crate::{
    ext::*,
    time::{Duration, Instant},
};
use std::{marker::PhantomData, task::Context};
use tracing::{debug_span, Instrument};

pub trait Latency<T> {
    fn for_value(&self, value: &T) -> Duration;
}

impl<T> Latency<T> for Duration {
    fn for_value(&self, _value: &T) -> Duration {
        *self
    }
}

pub struct Queue<T, Q, L> {
    inner: Q,
    latency: L,
    value: PhantomData<T>,
}

impl<T, Q, L> Queue<T, Q, L> {
    pub fn new(inner: Q, latency: L) -> Self {
        Self {
            inner,
            latency,
            value: PhantomData,
        }
    }

    pub fn inner(&self) -> &Q {
        &self.inner
    }
}

impl<T, Q, L> Queue<T, Q, L>
where
    Q: super::Conditional<(Instant, T)>,
    L: Latency<T>,
{
    fn push_with_latency(&self, value: T) -> Result<(Instant, Option<T>), PushError<T>> {
        let latency = self.latency.for_value(&value);
        let target = Instant::now() + latency;
        let value = (target, value);

        match self.inner.push(value) {
            Ok(None) => Ok((target, None)),
            Ok(Some((_t, value))) => Ok((target, Some(value))),
            Err(PushError::Closed((_, value))) => Err(PushError::Closed(value)),
            Err(PushError::Full((_, value))) => Err(PushError::Full(value)),
        }
    }
}

impl<T, Q, L> super::Queue<T> for Queue<T, Q, L>
where
    Q: super::Conditional<(Instant, T)>,
    L: Latency<T>,
    T: 'static + Sync + Send,
{
    fn push(&self, value: T) -> Result<Option<T>, PushError<T>> {
        let (_target, value) = self.push_with_latency(value)?;
        Ok(value)
    }

    fn push_with_context(&self, value: T, cx: &mut Context) -> Result<Option<T>, PushError<T>> {
        let (target, value) = self.push_with_latency(value)?;

        let waker = cx.waker().clone();
        async move {
            crate::time::sleep_until(target).await;
            waker.wake();
        }
        .instrument(debug_span!("message"))
        .spawn();

        Ok(value)
    }

    fn pop(&self) -> Result<T, PopError> {
        let (_t, value) = self.inner.find_pop(|(t, _)| t.has_elapsed())?;
        Ok(value)
    }

    fn pop_with_context(&self, cx: &mut Context) -> Result<T, PopError> {
        let value = self.pop()?;
        cx.waker().wake_by_ref();
        Ok(value)
    }

    fn close(&self) -> Result<(), CloseError> {
        self.inner.close()
    }

    fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn is_full(&self) -> bool {
        self.inner.is_full()
    }

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn capacity(&self) -> Option<usize> {
        self.inner.capacity()
    }
}
