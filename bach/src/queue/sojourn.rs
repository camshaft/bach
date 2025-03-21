use super::{CloseError, PopError, PushError, Pushable};
use crate::time::Instant;
use core::fmt;
use std::{marker::PhantomData, task::Context};

pub struct Queue<T, Q> {
    inner: Q,
    value: PhantomData<T>,
}

impl<T, Q: Default> Default for Queue<T, Q> {
    fn default() -> Self {
        Self::new(Q::default())
    }
}

impl<T, Q: fmt::Debug> fmt::Debug for Queue<T, Q> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T, Q> Queue<T, Q> {
    pub fn new(inner: Q) -> Self {
        Self {
            inner,
            value: PhantomData,
        }
    }

    pub fn inner(&self) -> &Q {
        &self.inner
    }
}

struct WithTimestamp<'a, T>(&'a mut dyn Pushable<T>);

impl<T> Pushable<(Instant, T)> for WithTimestamp<'_, T> {
    fn produce(&mut self) -> (Instant, T) {
        let value = self.0.produce();
        (Instant::now(), value)
    }
}

impl<T, Q> super::Queue<T> for Queue<T, Q>
where
    Q: super::Queue<(Instant, T)>,
{
    fn push_lazy(&mut self, value: &mut dyn Pushable<T>) -> Result<Option<T>, PushError> {
        let mut value = WithTimestamp(value);
        match self.inner.push_lazy(&mut value)? {
            None => Ok(None),
            Some((t, value)) => {
                measure!("sojourn_time", t.elapsed());
                Ok(Some(value))
            }
        }
    }

    fn push_with_notify(
        &mut self,
        value: &mut dyn Pushable<T>,
        cx: &mut Context,
    ) -> Result<Option<T>, PushError> {
        let mut value = WithTimestamp(value);
        match self.inner.push_with_notify(&mut value, cx)? {
            None => Ok(None),
            Some((t, value)) => {
                measure!("sojourn_time", t.elapsed());
                Ok(Some(value))
            }
        }
    }

    fn pop(&mut self) -> Result<T, PopError> {
        let (t, value) = self.inner.pop()?;
        measure!("sojourn_time", t.elapsed());
        Ok(value)
    }

    fn pop_with_notify(&mut self, cx: &mut Context) -> Result<T, PopError> {
        let (t, value) = self.inner.pop_with_notify(cx)?;
        measure!("sojourn_time", t.elapsed());
        Ok(value)
    }

    fn close(&mut self) -> Result<(), CloseError> {
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

impl<T, Q> super::Conditional<T> for Queue<T, Q>
where
    Q: super::Conditional<(Instant, T)>,
{
    fn find_pop<F: Fn(&T) -> bool>(&mut self, check: F) -> Result<T, PopError> {
        let (t, value) = self.inner.find_pop(|(_, value)| check(value))?;
        measure!("sojourn_time", t.elapsed());
        Ok(value)
    }
}
