use crate::{
    environment::net::pcap::{Record, Writer},
    queue::{CloseError, PopError, PushError, Pushable, Queue},
};
use core::marker::PhantomData;
use std::{io, task::Context};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Push,
    Pop,
}

pub trait QueueExt<T>
where
    Self: Sized + Queue<T>,
    T: Record,
{
    fn pcap<O: io::Write>(self, out: Writer<O>, direction: Direction) -> PcapQueue<Self, O, T> {
        PcapQueue::new(self, out, direction)
    }

    fn pcap_push<O: io::Write>(self, out: Writer<O>) -> PcapQueue<Self, O, T> {
        self.pcap(out, Direction::Push)
    }

    fn pcap_pop<O: io::Write>(self, out: Writer<O>) -> PcapQueue<Self, O, T> {
        self.pcap(out, Direction::Pop)
    }
}

impl<Q, T> QueueExt<T> for Q
where
    Q: Queue<T>,
    T: Record,
{
}

pub struct PcapQueue<Q, O, T>
where
    Q: Queue<T>,
    O: io::Write,
    T: Record,
{
    queue: Q,
    pcap: Writer<O>,
    direction: Direction,
    value: PhantomData<T>,
}

impl<Q, O, T> PcapQueue<Q, O, T>
where
    Q: Queue<T>,
    O: io::Write,
    T: Record,
{
    pub fn new(queue: Q, out: Writer<O>, direction: Direction) -> Self {
        PcapQueue {
            queue,
            pcap: out,
            direction,
            value: PhantomData,
        }
    }
}

impl<Q, O, T> Queue<T> for PcapQueue<Q, O, T>
where
    Q: Queue<T>,
    O: io::Write,
    T: Record,
{
    fn push_lazy(&mut self, value: &mut dyn Pushable<T>) -> Result<Option<T>, PushError> {
        if self.direction == Direction::Pop {
            return self.queue.push_lazy(value);
        }
        let mut value = PushablePacket {
            inner: value,
            writer: &mut self.pcap,
        };
        self.queue.push_lazy(&mut value)
    }

    fn push_with_notify(
        &mut self,
        value: &mut dyn Pushable<T>,
        cx: &mut Context,
    ) -> Result<Option<T>, PushError> {
        if self.direction == Direction::Pop {
            return self.queue.push_with_notify(value, cx);
        }
        let mut value = PushablePacket {
            inner: value,
            writer: &mut self.pcap,
        };
        self.queue.push_with_notify(&mut value, cx)
    }

    fn pop(&mut self) -> Result<T, PopError> {
        let mut packet = self.queue.pop()?;
        if self.direction == Direction::Pop {
            packet
                .write_pcap_record(&mut self.pcap)
                .expect("failed to write pcap");
        }
        Ok(packet)
    }

    fn pop_with_notify(&mut self, cx: &mut Context) -> Result<T, PopError> {
        let mut packet = self.queue.pop_with_notify(cx)?;
        if self.direction == Direction::Pop {
            packet
                .write_pcap_record(&mut self.pcap)
                .expect("failed to write pcap");
        }
        Ok(packet)
    }

    fn close(&mut self) -> Result<(), CloseError> {
        self.queue.close()
    }

    fn is_closed(&self) -> bool {
        self.queue.is_closed()
    }

    fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    fn is_full(&self) -> bool {
        self.queue.is_full()
    }

    fn len(&self) -> usize {
        self.queue.len()
    }

    fn capacity(&self) -> Option<usize> {
        self.queue.capacity()
    }
}

struct PushablePacket<'a, O, T> {
    inner: &'a mut dyn Pushable<T>,
    writer: &'a mut Writer<O>,
}

impl<O, T> Pushable<T> for PushablePacket<'_, O, T>
where
    O: io::Write,
    T: Record,
{
    fn produce(&mut self) -> T {
        let mut packet = self.inner.produce();

        packet
            .write_pcap_record(self.writer)
            .expect("failed to write pcap");

        packet
    }
}
