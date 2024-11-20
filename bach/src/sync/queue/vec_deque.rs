use super::{CloseError, PopError, PushError};
use alloc::{collections::VecDeque, sync::Arc};
use core::fmt;
use std::sync::Mutex;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Discipline {
    #[default]
    Fifo,
    Lifo,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overflow {
    #[default]
    PreferRecent,
    PreferOldest,
}

#[derive(Default)]
pub struct Builder {
    capacity: Option<usize>,
    discipline: Discipline,
    overflow: Overflow,
}

impl Builder {
    pub fn with_capacity(mut self, capacity: Option<usize>) -> Self {
        self.capacity = capacity.map(|v| v.max(1));
        self
    }

    pub fn with_discipline(mut self, discipline: Discipline) -> Self {
        self.discipline = discipline;
        self
    }

    pub fn with_overflow(mut self, overflow: Overflow) -> Self {
        self.overflow = overflow;
        self
    }

    pub fn build<T>(self) -> Queue<T> {
        let config = Config {
            capacity: self.capacity,
            discipline: self.discipline,
            overflow: self.overflow,
        };
        let queue = if let Some(cap) = self.capacity {
            VecDeque::with_capacity(cap)
        } else {
            VecDeque::new()
        };
        let queue = Mutex::new((queue, true));
        Queue(Arc::new(Inner { config, queue }))
    }
}

struct Config {
    capacity: Option<usize>,
    discipline: Discipline,
    overflow: Overflow,
}

impl Config {
    #[inline]
    fn push<T>(&self, queue: &mut VecDeque<T>, value: T) -> Result<Option<T>, PushError<T>> {
        let mut prev = None;

        if self.is_full(queue) {
            match self.overflow {
                Overflow::PreferOldest => return Err(PushError::Full(value)),
                Overflow::PreferRecent => {
                    // shift the queue items around to make room for our new value
                    prev = match self.discipline {
                        Discipline::Fifo => queue.pop_front(),
                        Discipline::Lifo => queue.pop_back(),
                    };
                }
            }
        }

        match self.discipline {
            Discipline::Fifo => queue.push_back(value),
            Discipline::Lifo => queue.push_front(value),
        }

        Ok(prev)
    }

    #[inline]
    fn is_full<T>(&self, queue: &VecDeque<T>) -> bool {
        if let Some(cap) = self.capacity {
            queue.len() >= cap
        } else {
            false
        }
    }
}

struct Inner<T> {
    config: Config,
    queue: Mutex<(VecDeque<T>, bool)>,
}

pub struct Queue<T>(Arc<Inner<T>>);

impl<T> Default for Queue<T> {
    fn default() -> Self {
        Builder::default().build()
    }
}

impl<T> Clone for Queue<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> fmt::Debug for Queue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Queue").finish_non_exhaustive()
    }
}

impl Queue<()> {
    pub fn builder() -> Builder {
        Builder::default()
    }
}

impl<T> Queue<T> {
    pub fn drain(&self) -> VecDeque<T> {
        if let Ok(mut inner) = self.0.queue.lock() {
            let replacement = VecDeque::with_capacity(inner.0.capacity());
            core::mem::replace(&mut inner.0, replacement)
        } else {
            VecDeque::new()
        }
    }
}

impl<T> super::Queue<T> for Queue<T> {
    fn push(&self, value: T) -> Result<Option<T>, PushError<T>> {
        let Some(mut inner) = self.0.queue.lock().ok().filter(|v| v.1) else {
            return Err(PushError::Closed(value));
        };

        self.0.config.push(&mut inner.0, value)
    }

    fn pop(&self) -> Result<T, PopError> {
        let mut inner = self
            .0
            .queue
            .lock()
            .ok()
            .filter(|v| v.1)
            .ok_or(PopError::Closed)?;

        inner.0.pop_front().ok_or(PopError::Empty)
    }

    fn close(&self) -> Result<(), super::CloseError> {
        let mut inner = self.0.queue.lock().map_err(|_| CloseError::AlreadyClosed)?;
        let prev = core::mem::replace(&mut inner.1, false);
        if prev {
            Ok(())
        } else {
            Err(CloseError::AlreadyClosed)
        }
    }

    fn is_closed(&self) -> bool {
        self.0.queue.lock().map_or(true, |l| l.1)
    }

    fn is_empty(&self) -> bool {
        self.0.queue.lock().map_or(true, |l| l.0.is_empty() || !l.1)
    }

    fn is_full(&self) -> bool {
        self.0
            .queue
            .lock()
            .map_or(false, |l| self.0.config.is_full(&l.0))
    }

    fn len(&self) -> usize {
        self.0.queue.lock().map_or(0, |l| l.0.len())
    }

    fn capacity(&self) -> Option<usize> {
        self.0.config.capacity
    }
}
