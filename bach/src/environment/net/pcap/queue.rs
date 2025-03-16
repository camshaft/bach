use crate::{
    environment::net::{ip::Packet, pcap::Writer},
    queue::{CloseError, PopError, PushError, Pushable, Queue},
};
use std::{io, task::Context};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Push,
    Pop,
}

pub trait QueueExt
where
    Self: Sized + Queue<Packet>,
{
    fn pcap<O: io::Write>(self, out: Writer<O>, direction: Direction) -> PcapQueue<Self, O> {
        PcapQueue::new(self, out, direction)
    }

    fn pcap_push<O: io::Write>(self, out: Writer<O>) -> PcapQueue<Self, O> {
        self.pcap(out, Direction::Push)
    }

    fn pcap_pop<O: io::Write>(self, out: Writer<O>) -> PcapQueue<Self, O> {
        self.pcap(out, Direction::Pop)
    }
}

impl<Q: Queue<Packet>> QueueExt for Q {}

pub struct PcapQueue<Q, O>
where
    Q: Queue<Packet>,
    O: io::Write,
{
    queue: Q,
    pcap: Writer<O>,
    direction: Direction,
}

impl<Q, O> PcapQueue<Q, O>
where
    Q: Queue<Packet>,
    O: io::Write,
{
    pub fn new(queue: Q, out: Writer<O>, direction: Direction) -> Self {
        PcapQueue {
            queue,
            pcap: out,
            direction,
        }
    }
}

impl<Q, O> Queue<Packet> for PcapQueue<Q, O>
where
    Q: Queue<Packet>,
    O: io::Write,
{
    fn push_lazy(&mut self, value: &mut dyn Pushable<Packet>) -> Result<Option<Packet>, PushError> {
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
        value: &mut dyn Pushable<Packet>,
        cx: &mut Context,
    ) -> Result<Option<Packet>, PushError> {
        if self.direction == Direction::Pop {
            return self.queue.push_with_notify(value, cx);
        }
        let mut value = PushablePacket {
            inner: value,
            writer: &mut self.pcap,
        };
        self.queue.push_with_notify(&mut value, cx)
    }

    fn pop(&mut self) -> Result<Packet, PopError> {
        let mut packet = self.queue.pop()?;
        if self.direction == Direction::Pop {
            self.pcap
                .write_packet(&mut packet)
                .expect("failed to write pcap");
        }
        Ok(packet)
    }

    fn pop_with_notify(&mut self, cx: &mut Context) -> Result<Packet, PopError> {
        let mut packet = self.queue.pop_with_notify(cx)?;
        if self.direction == Direction::Pop {
            self.pcap
                .write_packet(&mut packet)
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

struct PushablePacket<'a, O> {
    inner: &'a mut dyn Pushable<Packet>,
    writer: &'a mut Writer<O>,
}

impl<O> Pushable<Packet> for PushablePacket<'_, O>
where
    O: io::Write,
{
    fn produce(&mut self) -> Packet {
        let mut packet = self.inner.produce();

        self.writer
            .write_packet(&mut packet)
            .expect("failed to write pcap");

        packet
    }
}
