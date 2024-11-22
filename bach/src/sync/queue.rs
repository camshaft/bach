use crate::{sync::channel, time::Instant};
use core::fmt;
use std::{sync::Arc, task::Context};

pub mod latent;
pub mod priority;
pub mod sojourn;
pub mod span;
pub mod vec_deque;

pub trait Queue<T> {
    fn push(&self, value: T) -> Result<Option<T>, PushError<T>>;
    fn push_with_context(&self, value: T, cx: &mut Context) -> Result<Option<T>, PushError<T>>;

    fn pop(&self) -> Result<T, PopError>;
    fn pop_with_context(&self, cx: &mut Context) -> Result<T, PopError>;

    fn close(&self) -> Result<(), CloseError>;
    fn is_closed(&self) -> bool;
    fn is_empty(&self) -> bool;
    fn is_full(&self) -> bool;
    fn len(&self) -> usize;
    fn capacity(&self) -> Option<usize>;
}

impl<T, Q> Queue<T> for Arc<Q>
where
    Q: Queue<T>,
{
    fn push(&self, value: T) -> Result<Option<T>, PushError<T>> {
        self.as_ref().push(value)
    }

    fn push_with_context(&self, value: T, cx: &mut Context) -> Result<Option<T>, PushError<T>> {
        self.as_ref().push_with_context(value, cx)
    }

    fn pop(&self) -> Result<T, PopError> {
        self.as_ref().pop()
    }

    fn pop_with_context(&self, cx: &mut Context) -> Result<T, PopError> {
        self.as_ref().pop_with_context(cx)
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

pub trait Conditional<T>: Queue<T> {
    fn find_pop<F: Fn(&T) -> bool>(&self, check: F) -> Result<T, PopError>;
}

pub trait QueueExt<T>: 'static + Queue<T> + Sized + Send + Sync {
    #[inline]
    fn span(self, name: &'static str) -> span::Queue<Self> {
        span::Queue::new(self, name)
    }

    #[inline]
    fn channel(self) -> (channel::Sender<T>, channel::Receiver<T>) {
        channel::new(self)
    }
}

impl<Q, T> QueueExt<T> for Q where Q: 'static + Queue<T> + Sized + Send + Sync {}

pub trait InstantQueueExt<T>: 'static + Queue<(Instant, T)> + Sized + Send + Sync {
    #[inline]
    fn sojourn(self) -> sojourn::Queue<T, Self> {
        sojourn::Queue::new(self)
    }

    #[inline]
    fn latent<L>(self, latency: L) -> latent::Queue<T, Self, L>
    where
        L: latent::Latency<T>,
        Self: Conditional<(Instant, T)>,
    {
        latent::Queue::new(self, latency)
    }
}

impl<Q, T> InstantQueueExt<T> for Q where Q: 'static + Queue<(Instant, T)> + Sized + Send + Sync {}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PushError<T> {
    Full(T),
    Closed(T),
}

impl<T> PushError<T> {
    /// Unwraps the message that couldn't be sent.
    pub fn into_inner(self) -> T {
        match self {
            Self::Full(t) => t,
            Self::Closed(t) => t,
        }
    }

    /// Returns `true` if the queue is full but not closed.
    pub fn is_full(&self) -> bool {
        match self {
            Self::Full(_) => true,
            Self::Closed(_) => false,
        }
    }

    /// Returns `true` if the queue is closed.
    pub fn is_closed(&self) -> bool {
        match self {
            Self::Full(_) => false,
            Self::Closed(_) => true,
        }
    }
}

impl<T> std::error::Error for PushError<T> {}

impl<T> fmt::Debug for PushError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            PushError::Full(..) => write!(f, "Full(..)"),
            PushError::Closed(..) => write!(f, "Closed(..)"),
        }
    }
}

impl<T> fmt::Display for PushError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            PushError::Full(..) => write!(f, "sending into a full queue"),
            PushError::Closed(..) => write!(f, "sending into a closed queue"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopError {
    Empty,
    Closed,
}

impl PopError {
    /// Returns `true` if the queue is empty but not closed.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Empty => true,
            Self::Closed => false,
        }
    }

    /// Returns `true` if the queue is empty and closed.
    pub fn is_closed(&self) -> bool {
        match self {
            Self::Empty => false,
            Self::Closed => true,
        }
    }
}

impl std::error::Error for PopError {}

impl fmt::Display for PopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Empty => write!(f, "receiving from an empty queue"),
            Self::Closed => write!(f, "receiving from an empty and closed queue"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseError {
    AlreadyClosed,
}

impl std::error::Error for CloseError {}

impl fmt::Display for CloseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::AlreadyClosed => write!(f, "queue already closed"),
        }
    }
}
