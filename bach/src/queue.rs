use crate::time::Instant;
use core::fmt;
use std::{sync::Mutex, task::Context};

pub mod latent;
pub mod priority;
pub mod sojourn;
pub mod span;
pub mod vec_deque;

pub trait Pushable<T> {
    fn produce(&mut self) -> T;
}

impl<T> Pushable<T> for Option<T> {
    #[inline]
    fn produce(&mut self) -> T {
        self.take().expect("cannot produce multiple messages")
    }
}

impl<A, At, B, Bt> Pushable<(At, Bt)> for (A, B)
where
    A: Pushable<At>,
    B: Pushable<Bt>,
{
    #[inline]
    fn produce(&mut self) -> (At, Bt) {
        let a = self.0.produce();
        let b = self.1.produce();
        (a, b)
    }
}

pub trait Queue<T> {
    fn push_lazy(&mut self, value: &mut dyn Pushable<T>) -> Result<Option<T>, PushError>;

    /// Pushes `value` into the queue while returning it as an error if the operation fails
    #[inline]
    fn push(&mut self, value: T) -> Result<Option<T>, (PushError, T)> {
        let mut value = Some(value);
        match self.push_lazy(&mut value) {
            Ok(v) => Ok(v),
            Err(e) => Err((e, value.unwrap())),
        }
    }

    fn push_with_notify(
        &mut self,
        value: &mut dyn Pushable<T>,
        cx: &mut Context,
    ) -> Result<Option<T>, PushError>;

    fn pop(&mut self) -> Result<T, PopError>;
    fn pop_with_notify(&mut self, cx: &mut Context) -> Result<T, PopError>;

    fn close(&mut self) -> Result<(), CloseError>;
    fn is_closed(&self) -> bool;
    fn is_empty(&self) -> bool;
    fn is_full(&self) -> bool;
    fn len(&self) -> usize;
    fn capacity(&self) -> Option<usize>;

    #[inline]
    fn check_push(&self) -> Result<(), PushError> {
        if self.is_closed() {
            Err(PushError::Closed)
        } else if self.is_full() {
            Err(PushError::Full)
        } else {
            Ok(())
        }
    }
}

pub trait Conditional<T>: Queue<T> {
    fn find_pop<F: Fn(&T) -> bool>(&mut self, check: F) -> Result<T, PopError>;
}

pub trait QueueExt<T>: 'static + Queue<T> + Sized + Send {
    #[inline]
    fn span<N: Into<alloc::borrow::Cow<'static, str>>>(self, name: N) -> span::Queue<Self> {
        span::Queue::new(self, name)
    }

    #[inline]
    fn mutex(self) -> Mutex<Self> {
        Mutex::new(self)
    }
}

impl<Q, T> QueueExt<T> for Q where Q: 'static + Queue<T> + Sized + Send {}

pub trait InstantQueueExt<T>: 'static + Queue<(Instant, T)> + Sized + Send {
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

impl<Q, T> InstantQueueExt<T> for Q where Q: 'static + Queue<(Instant, T)> + Sized + Send {}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PushError {
    Full,
    Closed,
}

impl PushError {
    /// Returns `true` if the queue is full but not closed.
    pub fn is_full(&self) -> bool {
        match self {
            Self::Full => true,
            Self::Closed => false,
        }
    }

    /// Returns `true` if the queue is closed.
    pub fn is_closed(&self) -> bool {
        match self {
            Self::Full => false,
            Self::Closed => true,
        }
    }
}

impl From<PushError> for std::io::Error {
    fn from(err: PushError) -> Self {
        let kind: std::io::ErrorKind = err.into();
        kind.into()
    }
}

impl From<PushError> for std::io::ErrorKind {
    fn from(err: PushError) -> Self {
        match err {
            PushError::Full => std::io::ErrorKind::WouldBlock,
            PushError::Closed => std::io::ErrorKind::BrokenPipe,
        }
    }
}

impl std::error::Error for PushError {}

impl fmt::Debug for PushError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            PushError::Full => write!(f, "Full"),
            PushError::Closed => write!(f, "Closed"),
        }
    }
}

impl fmt::Display for PushError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            PushError::Full => write!(f, "sending into a full queue"),
            PushError::Closed => write!(f, "sending into a closed queue"),
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

impl From<PopError> for std::io::Error {
    fn from(err: PopError) -> Self {
        let kind: std::io::ErrorKind = err.into();
        kind.into()
    }
}

impl From<PopError> for std::io::ErrorKind {
    fn from(err: PopError) -> Self {
        match err {
            PopError::Empty => std::io::ErrorKind::WouldBlock,
            PopError::Closed => std::io::ErrorKind::BrokenPipe,
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
