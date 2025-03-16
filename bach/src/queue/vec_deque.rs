use super::{CloseError, PopError, PushError, Pushable};
use alloc::collections::VecDeque;
use core::fmt;
use std::task::Context;

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
        Queue {
            config,
            queue,
            is_open: true,
        }
    }
}

struct Config {
    capacity: Option<usize>,
    discipline: Discipline,
    overflow: Overflow,
}

impl Config {
    #[inline]
    fn push<T, P: Pushable<T> + ?Sized>(
        &self,
        queue: &mut VecDeque<T>,
        value: &mut P,
    ) -> Result<Option<T>, PushError> {
        let mut prev = None;

        if self.is_full(queue) {
            match self.overflow {
                Overflow::PreferOldest => {
                    count!(
                        "full",
                        "discipline" = self.discipline.as_str(),
                        "overflow" = self.overflow.as_str(),
                    );

                    return Err(PushError::Full);
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

        let value = value.produce();
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
    queue: VecDeque<T>,
    is_open: bool,
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
    pub fn drain(&mut self) -> VecDeque<T> {
        count!(
            "drain",
            "discipline" = self.config.discipline.as_str(),
            "overflow" = self.config.overflow.as_str()
        );

        let replacement = VecDeque::with_capacity(self.queue.capacity());
        self.config.record_len(&replacement);
        core::mem::replace(&mut self.queue, replacement)
    }
}

impl<T> super::Queue<T> for Queue<T> {
    fn push_lazy(&mut self, value: &mut dyn Pushable<T>) -> Result<Option<T>, PushError> {
        self.config.push(&mut self.queue, value)
    }

    fn push_with_notify(
        &mut self,
        value: &mut dyn Pushable<T>,
        cx: &mut Context,
    ) -> Result<Option<T>, PushError> {
        let value = self.push_lazy(value)?;
        cx.waker().wake_by_ref();
        Ok(value)
    }

    fn pop(&mut self) -> Result<T, PopError> {
        if self.queue.is_empty() && !self.is_open {
            return Err(PopError::Closed);
        }

        let value = self.queue.pop_front().ok_or(PopError::Empty)?;

        count!(
            "pop",
            "discipline" = self.config.discipline.as_str(),
            "overflow" = self.config.overflow.as_str(),
        );

        self.config.record_len(&self.queue);

        Ok(value)
    }

    fn pop_with_notify(&mut self, cx: &mut Context) -> Result<T, PopError> {
        let value = self.pop()?;
        cx.waker().wake_by_ref();
        Ok(value)
    }

    fn close(&mut self) -> Result<(), super::CloseError> {
        let prev = core::mem::replace(&mut self.is_open, false);
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
        !self.is_open
    }

    fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    fn is_full(&self) -> bool {
        self.config.is_full(&self.queue)
    }

    fn len(&self) -> usize {
        self.queue.len()
    }

    fn capacity(&self) -> Option<usize> {
        self.config.capacity
    }
}

impl<T> super::Conditional<T> for Queue<T> {
    #[inline]
    fn find_pop<F: Fn(&T) -> bool>(&mut self, check: F) -> Result<T, PopError> {
        if self.queue.is_empty() && !self.is_open {
            return Err(PopError::Closed);
        }

        let queue = &mut self.queue;

        let mut selected = None;
        for (idx, value) in queue.iter().enumerate() {
            if check(value) {
                selected = Some(idx);
                break;
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
