use super::{CloseError, PopError, PushError};
use alloc::collections::VecDeque;
use core::fmt;
use std::{sync::Mutex, task::Context};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Discipline {
    #[default]
    Fifo,
    Lifo,
}

impl Discipline {
    pub fn as_str(&self) -> &'static str {
        match self {
            Discipline::Fifo => "fifo",
            Discipline::Lifo => "lifo",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overflow {
    PreferRecent,
    #[default]
    PreferOldest,
}

impl Overflow {
    pub fn as_str(&self) -> &'static str {
        match self {
            Overflow::PreferRecent => "prefer_recent",
            Overflow::PreferOldest => "prefer_oldest",
        }
    }
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
        Queue { config, queue }
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
                Overflow::PreferOldest => {
                    count!(
                        "full",
                        "discipline" = self.discipline.as_str(),
                        "overflow" = self.overflow.as_str(),
                    );

                    return Err(PushError::Full(value));
                }
                Overflow::PreferRecent => {
                    count!(
                        "shift",
                        "discipline" = self.discipline.as_str(),
                        "overflow" = self.overflow.as_str(),
                    );

                    // shift the queue items around to make room for our new value
                    prev = match self.discipline {
                        Discipline::Fifo => queue.pop_front(),
                        Discipline::Lifo => queue.pop_back(),
                    };
                }
            }
        }

        count!(
            "push",
            "discipline" = self.discipline.as_str(),
            "overflow" = self.overflow.as_str(),
        );

        match self.discipline {
            Discipline::Fifo => queue.push_back(value),
            Discipline::Lifo => queue.push_front(value),
        }

        self.record_len(queue);

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

    #[inline]
    fn record_len<T>(&self, queue: &VecDeque<T>) {
        measure!(
            "len",
            queue.len() as u32,
            "discipline" = self.discipline.as_str(),
            "overflow" = self.overflow.as_str(),
        );
    }
}

pub struct Queue<T> {
    config: Config,
    queue: Mutex<(VecDeque<T>, bool)>,
}

impl<T> Default for Queue<T> {
    fn default() -> Self {
        Builder::default().build()
    }
}

impl<T> fmt::Debug for Queue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("vec_deque::Queue").finish_non_exhaustive()
    }
}

impl Queue<()> {
    pub fn builder() -> Builder {
        Builder::default()
    }
}

impl<T> Queue<T> {
    pub fn drain(&self) -> VecDeque<T> {
        count!(
            "drain",
            "discipline" = self.config.discipline.as_str(),
            "overflow" = self.config.overflow.as_str()
        );

        if let Ok(mut inner) = self.queue.lock() {
            let replacement = VecDeque::with_capacity(inner.0.capacity());
            self.config.record_len(&replacement);
            core::mem::replace(&mut inner.0, replacement)
        } else {
            VecDeque::new()
        }
    }
}

impl<T> super::Queue<T> for Queue<T> {
    fn push(&self, value: T) -> Result<Option<T>, PushError<T>> {
        let Some(mut inner) = self.queue.lock().ok().filter(|v| v.1) else {
            return Err(PushError::Closed(value));
        };

        self.config.push(&mut inner.0, value)
    }

    fn push_with_context(&self, value: T, cx: &mut Context) -> Result<Option<T>, PushError<T>> {
        let value = self.push(value)?;
        cx.waker().wake_by_ref();
        Ok(value)
    }

    fn pop(&self) -> Result<T, PopError> {
        let mut inner = self
            .queue
            .lock()
            .ok()
            .filter(|v| !v.0.is_empty() || v.1)
            .ok_or(PopError::Closed)?;

        let value = inner.0.pop_front().ok_or(PopError::Empty)?;

        count!(
            "pop",
            "discipline" = self.config.discipline.as_str(),
            "overflow" = self.config.overflow.as_str(),
        );

        self.config.record_len(&inner.0);

        Ok(value)
    }

    fn pop_with_context(&self, cx: &mut Context) -> Result<T, PopError> {
        let value = self.pop()?;
        cx.waker().wake_by_ref();
        Ok(value)
    }

    fn close(&self) -> Result<(), super::CloseError> {
        let mut inner = self.queue.lock().map_err(|_| CloseError::AlreadyClosed)?;
        let prev = core::mem::replace(&mut inner.1, false);
        if prev {
            count!(
                "close",
                "discipline" = self.config.discipline.as_str(),
                "overflow" = self.config.overflow.as_str(),
            );

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

impl<T> super::Conditional<T> for Queue<T> {
    #[inline]
    fn find_pop<F: Fn(&T) -> bool>(&self, check: F) -> Result<T, PopError> {
        let mut inner = self
            .queue
            .lock()
            .ok()
            .filter(|v| !v.0.is_empty() || v.1)
            .ok_or(PopError::Closed)?;

        let queue = &mut inner.0;

        let mut selected = None;
        for (idx, value) in queue.iter().enumerate() {
            if check(value) {
                selected = Some(idx);
            }
        }

        let selected = selected.ok_or(PopError::Empty)?;
        let value = queue.remove(selected).unwrap();

        count!(
            "pop",
            "discipline" = self.config.discipline.as_str(),
            "overflow" = self.config.overflow.as_str(),
        );

        self.config.record_len(queue);

        Ok(value)
    }
}
