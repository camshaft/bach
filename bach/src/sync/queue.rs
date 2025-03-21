use crate::{
    queue::{CloseError, PopError, PushError, Pushable, Queue},
    sync::channel,
};
use std::{
    sync::{Arc, Mutex},
    task::Context,
};

pub trait Shared<T> {
    fn push_lazy(&self, value: &mut dyn Pushable<T>) -> Result<Option<T>, PushError>;
    fn push(&self, value: T) -> Result<Option<T>, (PushError, T)>;
    fn push_with_notify(
        &self,
        value: &mut dyn Pushable<T>,
        cx: &mut Context,
    ) -> Result<Option<T>, PushError>;

    fn pop(&self) -> Result<T, PopError>;
    fn pop_with_notify(&self, cx: &mut Context) -> Result<T, PopError>;

    fn close(&self) -> Result<(), CloseError>;
    fn is_closed(&self) -> bool;
    fn is_empty(&self) -> bool;
    fn is_full(&self) -> bool;
    fn len(&self) -> usize;
    fn capacity(&self) -> Option<usize>;
}

impl<T, Q> Shared<T> for Arc<Q>
where
    Q: Shared<T>,
{
    fn push_lazy(&self, value: &mut dyn Pushable<T>) -> Result<Option<T>, PushError> {
        self.as_ref().push_lazy(value)
    }

    fn push(&self, value: T) -> Result<Option<T>, (PushError, T)> {
        self.as_ref().push(value)
    }

    fn push_with_notify(
        &self,
        value: &mut dyn Pushable<T>,
        cx: &mut Context,
    ) -> Result<Option<T>, PushError> {
        self.as_ref().push_with_notify(value, cx)
    }

    fn pop(&self) -> Result<T, PopError> {
        self.as_ref().pop()
    }

    fn pop_with_notify(&self, cx: &mut Context) -> Result<T, PopError> {
        self.as_ref().pop_with_notify(cx)
    }

    fn close(&self) -> Result<(), CloseError> {
        self.as_ref().close()
    }

    fn is_closed(&self) -> bool {
        self.as_ref().is_closed()
    }

    fn is_empty(&self) -> bool {
        self.as_ref().is_empty()
    }

    fn is_full(&self) -> bool {
        self.as_ref().is_full()
    }

    fn len(&self) -> usize {
        self.as_ref().len()
    }

    fn capacity(&self) -> Option<usize> {
        self.as_ref().capacity()
    }
}

impl<T, Q> Shared<T> for Mutex<Q>
where
    Q: Queue<T>,
{
    fn push_lazy(&self, value: &mut dyn Pushable<T>) -> Result<Option<T>, PushError> {
        self.lock().map_err(|_| PushError::Closed)?.push_lazy(value)
    }

    fn push(&self, value: T) -> Result<Option<T>, (PushError, T)> {
        if let Ok(mut inner) = self.lock() {
            inner.push(value)
        } else {
            Err((PushError::Closed, value))
        }
    }

    fn push_with_notify(
        &self,
        value: &mut dyn Pushable<T>,
        cx: &mut Context,
    ) -> Result<Option<T>, PushError> {
        self.lock()
            .map_err(|_| PushError::Closed)?
            .push_with_notify(value, cx)
    }

    fn pop(&self) -> Result<T, PopError> {
        self.lock().map_err(|_| PopError::Closed)?.pop()
    }

    fn pop_with_notify(&self, cx: &mut Context) -> Result<T, PopError> {
        self.lock()
            .map_err(|_| PopError::Closed)?
            .pop_with_notify(cx)
    }

    fn close(&self) -> Result<(), CloseError> {
        self.lock().map_err(|_| CloseError::AlreadyClosed)?.close()
    }

    fn is_closed(&self) -> bool {
        self.lock().map_or(true, |q| q.is_closed())
    }

    fn is_empty(&self) -> bool {
        self.lock().map_or(true, |q| q.is_empty())
    }

    fn is_full(&self) -> bool {
        self.lock().map_or(true, |q| q.is_full())
    }

    fn len(&self) -> usize {
        self.lock().map_or(0, |q| q.len())
    }

    fn capacity(&self) -> Option<usize> {
        self.lock().map_or(Some(0), |q| q.capacity())
    }
}

pub trait SharedExt<T>: 'static + Shared<T> + Sized + Send + Sync {
    #[inline]
    fn channel(self) -> (channel::Sender<T>, channel::Receiver<T>) {
        channel::new(self)
    }
}

impl<Q, T> SharedExt<T> for Q where Q: 'static + Shared<T> + Sized + Send + Sync {}
