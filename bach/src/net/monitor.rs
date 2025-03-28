use crate::environment::net::registry::with_registry;
use core::fmt;
use std::{io, net::SocketAddr};

pub use crate::environment::net::ip::{transport::Kind as Transport, Packet};

use super::socket::SendOptions;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Command {
    /// Continues with the packet operation
    #[default]
    Pass,
    /// Drops the packet
    Drop,
}

impl Command {
    #[inline(always)]
    pub fn is_pass(self) -> bool {
        matches!(self, Self::Pass)
    }

    #[inline(always)]
    pub fn is_drop(self) -> bool {
        matches!(self, Self::Drop)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Send,
    Receive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DropReason {
    Monitor,
    UnknownDestination,
    ReceiveBufferFull,
}

#[derive(Clone, Copy)]
#[non_exhaustive]
pub struct SocketWrite<'a> {
    pub local_addr: &'a SocketAddr,
    pub peer_addr: &'a SocketAddr,
    pub transport: Transport,
    pub payload: &'a [io::IoSlice<'a>],
    pub opts: &'a SendOptions,
}

impl fmt::Debug for SocketWrite<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SocketWrite")
            .field("local_addr", &self.local_addr)
            .field("peer_addr", &self.peer_addr)
            .field("transport", &self.transport)
            .field("payload", &PayloadDebug(self.payload))
            .field("opts", &self.opts)
            .finish()
    }
}

struct PayloadDebug<'a, T>(&'a [T]);

impl<T: core::ops::Deref<Target = [u8]>> fmt::Debug for PayloadDebug<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "b\"")?;
        for chunk in self.0 {
            for &b in chunk.deref() {
                // https://doc.rust-lang.org/reference/tokens.html#byte-escapes
                if b == b'\n' {
                    write!(f, "\\n")?;
                } else if b == b'\r' {
                    write!(f, "\\r")?;
                } else if b == b'\t' {
                    write!(f, "\\t")?;
                } else if b == b'\\' || b == b'"' {
                    write!(f, "\\{}", b as char)?;
                } else if b == b'\0' {
                    write!(f, "\\0")?;
                // ASCII printable
                } else if (0x20..0x7f).contains(&b) {
                    write!(f, "{}", b as char)?;
                } else {
                    write!(f, "\\x{:02x}", b)?;
                }
            }
        }
        write!(f, "\"")?;
        Ok(())
    }
}

pub trait Monitor: 'static + Send {
    #[inline(always)]
    fn on_socket_opened(
        &mut self,
        local_addr: &SocketAddr,
        transport: Transport,
    ) -> io::Result<()> {
        let _ = local_addr;
        let _ = transport;
        Ok(())
    }

    #[inline(always)]
    fn on_socket_closed(&mut self, local_addr: &SocketAddr, transport: Transport) {
        let _ = local_addr;
        let _ = transport;
    }

    #[inline(always)]
    fn on_socket_write(&mut self, socket_write: &SocketWrite) -> io::Result<()> {
        let _ = socket_write;
        Ok(())
    }

    #[inline(always)]
    fn on_packet(&mut self, packet: &Packet, operation: Operation) -> Command {
        match operation {
            Operation::Send => self.on_packet_sent(packet),
            Operation::Receive => self.on_packet_received(packet),
        }
    }

    #[inline(always)]
    fn on_packet_received(&mut self, packet: &Packet) -> Command {
        let _ = packet;
        Command::Pass
    }

    #[inline(always)]
    fn on_packet_sent(&mut self, packet: &Packet) -> Command {
        let _ = packet;
        Command::Pass
    }

    #[inline(always)]
    fn on_packet_dropped(&mut self, packet: &Packet, drop_reason: DropReason) {
        let _ = packet;
        let _ = drop_reason;
    }
}

pub fn register<M: Monitor>(monitor: M) {
    with_registry(|r| {
        r.register_monitor(monitor);
        Ok(())
    })
    .expect("net registry not configured");
}

struct SocketWriteCb<F>(F);

impl<F> Monitor for SocketWriteCb<F>
where
    F: 'static + Send + FnMut(&SocketWrite) -> io::Result<()>,
{
    #[inline]
    fn on_socket_write(&mut self, socket_write: &SocketWrite) -> io::Result<()> {
        (self.0)(socket_write)
    }
}

pub fn on_socket_write<F>(f: F)
where
    F: 'static + Send + FnMut(&SocketWrite) -> io::Result<()>,
{
    register(SocketWriteCb(f));
}

struct PacketCallback<F>(F);

impl<F> Monitor for PacketCallback<F>
where
    F: 'static + Send + FnMut(&Packet, Operation) -> Command,
{
    #[inline]
    fn on_packet(&mut self, packet: &Packet, operation: Operation) -> Command {
        (self.0)(packet, operation)
    }
}

pub fn on_packet<F>(f: F)
where
    F: 'static + Send + FnMut(&Packet, Operation) -> Command,
{
    register(PacketCallback(f));
}

pub fn on_packet_sent<F>(mut f: F)
where
    F: 'static + Send + FnMut(&Packet) -> Command,
{
    on_packet(move |packet, operation| match operation {
        Operation::Send => f(packet),
        Operation::Receive => Command::Pass,
    })
}

pub fn on_packet_received<F>(mut f: F)
where
    F: 'static + Send + FnMut(&Packet) -> Command,
{
    on_packet(move |packet, operation| match operation {
        Operation::Receive => f(packet),
        Operation::Send => Command::Pass,
    })
}
