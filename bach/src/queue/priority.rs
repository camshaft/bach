use super::{CloseError, PopError, PushError, Pushable};
use alloc::collections::BinaryHeap;
use core::fmt;
use std::task::Context;

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
        Queue {
            config,
            queue,
            is_open: true,
        }
    }
}

struct Config {
    capacity: Option<usize>,
}

impl Config {
    #[inline]
    fn push<T, P: Pushable<T> + ?Sized>(
        &self,
        queue: &mut BinaryHeap<T>,
        value: &mut P,
    ) -> Result<Option<T>, PushError>
    where
        T: core::cmp::Ord,
    {
        if self.is_full(queue) {
            count!("full");

            return Err(PushError::Full);
        }

        count!("push");
        queue.push(value.produce());
        self.record_len(queue);

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

    #[inline]
    fn record_len<T>(&self, queue: &BinaryHeap<T>) {
        measure!("len", queue.len() as u32);
    }
}

pub struct Queue<T> {
    config: Config,
    queue: BinaryHeap<T>,
    is_open: bool,
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

        let value = self.queue.pop().ok_or(PopError::Empty)?;

        count!("pop");
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
            count!("close");
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
