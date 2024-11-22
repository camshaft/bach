use super::{CloseError, PopError, PushError};
use core::{fmt, ops};

pub struct Queue<Q> {
    name: &'static str,
    inner: Q,
}

impl<Q: Default> Default for Queue<Q> {
    fn default() -> Self {
        let name = "";
        let inner = Q::default();
        Self { name, inner }
    }
}

impl<Q: fmt::Debug> fmt::Debug for Queue<Q> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<Q> ops::Deref for Queue<Q> {
    type Target = Q;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<Q> Queue<Q> {
    pub fn new(inner: Q, name: &'static str) -> Self {
        Self { name, inner }
    }

    fn span(&self) -> tracing::Span {
        tracing::info_span!("queue", queue = %self.name)
    }
}

impl<T, Q> super::Queue<T> for Queue<Q>
where
    Q: super::Queue<T>,
{
    fn push(&self, value: T) -> Result<Option<T>, PushError<T>> {
        self.span().in_scope(|| self.inner.push(value))
    }

    fn pop(&self) -> Result<T, PopError> {
        self.span().in_scope(|| self.inner.pop())
    }

    fn close(&self) -> Result<(), CloseError> {
        self.span().in_scope(|| self.inner.close())
    }

    fn is_closed(&self) -> bool {
        self.span().in_scope(|| self.inner.is_closed())
    }

    fn is_empty(&self) -> bool {
        self.span().in_scope(|| self.inner.is_empty())
    }

    fn is_full(&self) -> bool {
        self.span().in_scope(|| self.inner.is_full())
    }

    fn len(&self) -> usize {
        self.span().in_scope(|| self.inner.len())
    }

    fn capacity(&self) -> Option<usize> {
        self.span().in_scope(|| self.inner.capacity())
    }
}
