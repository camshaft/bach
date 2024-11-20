use core::fmt;

pub mod vec_deque;

pub trait Queue<T> {
    fn push(&self, value: T) -> Result<Option<T>, PushError<T>>;
    fn pop(&self) -> Result<T, PopError>;
    fn close(&self) -> Result<(), CloseError>;
    fn is_closed(&self) -> bool;
    fn is_empty(&self) -> bool;
    fn is_full(&self) -> bool;
    fn len(&self) -> usize;
    fn capacity(&self) -> Option<usize>;
}

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
