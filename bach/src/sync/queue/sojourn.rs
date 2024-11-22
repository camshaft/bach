use super::{CloseError, PopError, PushError};
use crate::time::Instant;
use core::fmt;
use std::marker::PhantomData;

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

impl<T, Q> super::Queue<T> for Queue<T, Q>
where
    Q: super::Queue<(T, Instant)>,
{
    fn push(&self, value: T) -> Result<Option<T>, PushError<T>> {
        let value = (value, Instant::now());
        match self.inner.push(value) {
            Ok(None) => Ok(None),
            Ok(Some((value, t))) => {
                measure!("sojourn_time", t.elapsed());
                Ok(Some(value))
            }
            Err(PushError::Closed((value, _))) => Err(PushError::Closed(value)),
            Err(PushError::Full((value, _))) => Err(PushError::Full(value)),
        }
    }

    fn pop(&self) -> Result<T, PopError> {
        let (value, t) = self.inner.pop()?;
        measure!("sojourn_time", t.elapsed());
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
