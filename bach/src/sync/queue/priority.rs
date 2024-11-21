use super::{CloseError, PopError, PushError};
use alloc::collections::BinaryHeap;
use core::fmt;
use std::sync::Mutex;

#[derive(Default)]
pub struct Builder {
    capacity: Option<usize>,
}

impl Builder {
    pub fn with_capacity(mut self, capacity: Option<usize>) -> Self {
        self.capacity = capacity.map(|v| v.max(1));
        self
    }

    pub fn build<T>(self) -> Queue<T>
    where
        T: core::cmp::Ord,
    {
        let config = Config {
            capacity: self.capacity,
        };
        let queue = if let Some(cap) = self.capacity {
            BinaryHeap::with_capacity(cap)
        } else {
            BinaryHeap::new()
        };
        let queue = Mutex::new((queue, true));
        Queue { config, queue }
    }
}

struct Config {
    capacity: Option<usize>,
}

impl Config {
    #[inline]
    fn push<T>(&self, queue: &mut BinaryHeap<T>, value: T) -> Result<Option<T>, PushError<T>>
    where
        T: core::cmp::Ord,
    {
        if self.is_full(queue) {
            return Err(PushError::Full(value));
        }

        queue.push(value);

        Ok(None)
    }

    #[inline]
    fn is_full<T>(&self, queue: &BinaryHeap<T>) -> bool {
        if let Some(cap) = self.capacity {
            queue.len() >= cap
        } else {
            false
        }
    }
}

pub struct Queue<T> {
    config: Config,
    queue: Mutex<(BinaryHeap<T>, bool)>,
}

impl<T> Default for Queue<T>
where
    T: core::cmp::Ord,
{
    fn default() -> Self {
        Builder::default().build()
    }
}

impl<T> fmt::Debug for Queue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("priority::Queue").finish_non_exhaustive()
    }
}

impl Queue<()> {
    pub fn builder() -> Builder {
        Builder::default()
    }
}

impl<T> super::Queue<T> for Queue<T>
where
    T: core::cmp::Ord,
{
    fn push(&self, value: T) -> Result<Option<T>, PushError<T>> {
        let Some(mut inner) = self.queue.lock().ok().filter(|v| v.1) else {
            return Err(PushError::Closed(value));
        };

        self.config.push(&mut inner.0, value)
    }

    fn pop(&self) -> Result<T, PopError> {
        let mut inner = self
            .queue
            .lock()
            .ok()
            .filter(|v| v.1)
            .ok_or(PopError::Closed)?;

        inner.0.pop().ok_or(PopError::Empty)
    }

    fn close(&self) -> Result<(), super::CloseError> {
        let mut inner = self.queue.lock().map_err(|_| CloseError::AlreadyClosed)?;
        let prev = core::mem::replace(&mut inner.1, false);
        if prev {
            Ok(())
        } else {
            Err(CloseError::AlreadyClosed)
        }
    }

    fn is_closed(&self) -> bool {
        self.queue.lock().map_or(true, |l| l.1)
    }

    fn is_empty(&self) -> bool {
        self.queue.lock().map_or(true, |l| l.0.is_empty() || !l.1)
    }

    fn is_full(&self) -> bool {
        self.queue
            .lock()
            .map_or(false, |l| self.config.is_full(&l.0))
    }

    fn len(&self) -> usize {
        self.queue.lock().map_or(0, |l| l.0.len())
    }

    fn capacity(&self) -> Option<usize> {
        self.config.capacity
    }
}
