use crate::{
    environment::net::{
        ip::{header, transport, Category, Header, Packet, Segments},
        socket::{self, RecvOptions, RecvResult, SendOptions},
    },
    ext::*,
    net::SocketAddr,
    queue::Pushable,
    sync::channel,
};
use core::task::{Context, Poll};
use std::{io, sync::Mutex};

pub struct Socket {
    sender: Mutex<Sender>,
    receiver: Mutex<Receiver>,
    local_addr: SocketAddr,
    peer_addr: Mutex<Option<SocketAddr>>,
}

macro_rules! lock {
    ($lock:expr) => {
        $lock
            .lock()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{e}")))?
    };
}

impl Socket {
    pub fn new(
        sender: channel::Sender<Segments>,
        receiver: channel::Receiver<Packet>,
        local_addr: SocketAddr,
    ) -> Self {
        let sender = Mutex::new(Sender::new(sender));
        let receiver = Mutex::new(Receiver::new(receiver));
        Self {
            sender,
            receiver,
            local_addr,
            peer_addr: Mutex::new(None),
        }
    }
}

impl socket::Socket for Socket {
    fn poll_connect(&self, _cx: &mut Context, peer_addr: SocketAddr) -> Poll<io::Result<()>> {
        *lock!(self.peer_addr) = Some(peer_addr);
        Poll::Ready(Ok(()))
    }

    fn peer_addr(&self) -> io::Result<SocketAddr> {
        if let Some(peer_addr) = *lock!(self.peer_addr) {
            Ok(peer_addr)
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "Socket not connected",
            ))
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local_addr)
    }

    fn poll_writable(&self, _cx: &mut Context) -> Poll<io::Result<()>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "poll_writable isn't implemented",
        ))
        .into()
    }

    fn poll_readable(&self, _cx: &mut Context) -> Poll<io::Result<()>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "poll_readable isn't implemented",
        ))
        .into()
    }

    fn set_sockopt(&self, _option: super::Sockopt, _value: usize) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "set_sockopt isn't implemented",
        ))
    }

    fn get_sockopt(&self, _option: super::Sockopt) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "get_sockopt isn't implemented",
        ))
    }

    fn sendmsg(
        &self,
        cx: Option<&mut Context>,
        destination: &SocketAddr,
        payload: &[io::IoSlice],
        opts: SendOptions,
    ) -> io::Result<usize> {
        let peer_addr = *lock!(self.peer_addr);
        lock!(self.sender).sendmsg(cx, &self.local_addr, peer_addr, destination, payload, opts)
    }

    fn recvmsg(
        &self,
        cx: Option<&mut Context>,
        payload: &mut [io::IoSliceMut],
        opts: RecvOptions,
    ) -> io::Result<RecvResult> {
        let peer_addr = *lock!(self.peer_addr);
        lock!(self.receiver).recvmsg(cx, peer_addr, payload, opts)
    }

    fn shutdown(&self, how: std::net::Shutdown) -> io::Result<()> {
        // UDP doesn't have a shutdown method
        let _ = how;
        Ok(())
    }
}

struct Sender {
    channel: channel::Sender<Segments>,
    id: u16,
    ttl: u8,
}

impl Sender {
    fn new(channel: channel::Sender<Segments>) -> Self {
        Self {
            channel,
            id: 0,
            ttl: 64,
        }
    }

    fn sendmsg(
        &mut self,
        cx: Option<&mut Context>,
        local_addr: &SocketAddr,
        peer_addr: Option<SocketAddr>,
        destination: &SocketAddr,
        payload: &[io::IoSlice],
        opts: super::SendOptions,
    ) -> io::Result<usize> {
        let destination = if destination.is_unspecified() {
            peer_addr.as_ref().ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotConnected, "Socket not connected")
            })?
        } else {
            destination
        };

        let id = self.id;
        self.id = self.id.wrapping_add(1);
        let ttl = self.ttl;

        if opts.source.is_some() {
            todo!()
        }

        let header: Header = match (local_addr, destination) {
            (SocketAddr::V4(src), SocketAddr::V4(dst)) => header::V4 {
                source: *src.ip(),
                destination: *dst.ip(),
                dscp: 0,
                ecn: opts.ecn,
                df: true,
                id,
                ttl,
            }
            .into(),
            (SocketAddr::V6(src), SocketAddr::V4(dst)) => header::V6 {
                source: *src.ip(),
                destination: dst.ip().to_ipv6_mapped(),
                dscp: 0,
                ecn: opts.ecn,
                flow_label: 0,
                hop_limit: ttl,
            }
            .into(),
            (SocketAddr::V6(src), SocketAddr::V6(dst)) => header::V6 {
                source: *src.ip(),
                destination: *dst.ip(),
                dscp: 0,
                ecn: opts.ecn,
                flow_label: 0,
                hop_limit: ttl,
            }
            .into(),
            (SocketAddr::V4(_), SocketAddr::V6(_)) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "cannot send IPv6 packet on IPv4 socket",
                ))
            }
        };

        let transport = transport::Udp {
            source: local_addr.port(),
            destination: destination.port(),
            payload: Default::default(),
            checksum: 0,
        };

        let mut packet = SendablePacket {
            header,
            transport,
            payload,
            len: None,
            segment_len: opts.segment_len,
        };

        if let Some(cx) = cx {
            if self.channel.poll_push(cx, &mut packet)?.is_pending() {
                return Err(io::ErrorKind::WouldBlock.into());
            }
        } else {
            let mut channel = self.channel.clone();
            let packet = packet.produce();
            async move {
                let _ = channel.push(packet).await;
            }
            .spawn();
        }

        Ok(packet.len.unwrap_or(0))
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        let _ = self.channel.close();
    }
}

struct SendablePacket<'a> {
    header: Header,
    transport: transport::Udp,
    payload: &'a [io::IoSlice<'a>],
    len: Option<usize>,
    segment_len: Option<usize>,
}

impl Pushable<Segments> for SendablePacket<'_> {
    fn produce(&mut self) -> Segments {
        let len = if let Some(len) = self.len {
            len
        } else {
            let len = self.payload.iter().map(|p| p.len()).sum();
            self.len = Some(len);
            len
        };

        let mut payload = Vec::with_capacity(len);
        for chunk in self.payload {
            payload.extend_from_slice(chunk);
        }

        let mut transport = self.transport.clone();
        transport.payload = payload.into();

        let packet = Packet {
            header: self.header,
            transport: transport.into(),
        };

        let segment_len = self.segment_len.unwrap_or(len).min(len);

        Segments {
            packet,
            segment_len,
        }
    }
}

struct Receiver {
    channel: channel::Receiver<Packet>,
}

impl Receiver {
    fn new(channel: channel::Receiver<Packet>) -> Self {
        Self { channel }
    }

    fn recvmsg(
        &mut self,
        mut cx: Option<&mut Context>,
        peer_addr: Option<SocketAddr>,
        payload: &mut [io::IoSliceMut],
        opts: RecvOptions,
    ) -> io::Result<RecvResult> {
        if opts.peek {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "peek is not currently implemented",
            ));
        }

        loop {
            let packet = if let Some(cx) = cx.as_mut() {
                let res = self.channel.poll_pop(cx)?;
                let Poll::Ready(v) = res else {
                    return Err(io::ErrorKind::WouldBlock.into());
                };
                v
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "recvmsg without context is not currently implemented",
                ));
            };

            let destination = packet.destination();
            let source = packet.source();

            if let Some(peer_addr) = peer_addr {
                if source != peer_addr {
                    count!("peer_mismatch");
                    continue;
                }
            }

            let (copied_len, remaining_len) = packet.transport.copy_payload_into(payload);

            let res = RecvResult {
                peer_addr: source,
                local_addr: destination,
                ecn: packet.header.ecn(),
                len: copied_len,
                // TODO gro
                segment_len: copied_len,
                truncated: remaining_len > 0,
            };

            return Ok(res);
        }
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        let _ = self.channel.close();
    }
}
