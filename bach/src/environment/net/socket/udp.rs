use crate::{
    environment::net::{
        monitor::List as Monitors,
        registry,
        socket::{self, RecvOptions, RecvResult, SendOptions},
    },
    net::{
        monitor::{SocketRead, SocketWrite},
        SocketAddr,
    },
};
use core::{
    future::Future,
    pin::pin,
    task::{Context, Poll},
};
use std::{
    io,
    panic::{self, AssertUnwindSafe},
    sync::Mutex,
};
use turmoil_net::shim::tokio::net::UdpSocket as TurmoilUdpSocket;

pub struct Socket {
    inner: Option<TurmoilUdpSocket>,
    local_addr: SocketAddr,
    peer_addr: Mutex<Option<SocketAddr>>,
    monitors: Monitors,
}

macro_rules! lock {
    ($lock:expr) => {
        $lock.lock().map_err(|e| io::Error::other(format!("{e}")))?
    };
}

impl Socket {
    pub fn new(inner: TurmoilUdpSocket, monitors: Monitors) -> io::Result<Self> {
        let local_addr = inner.local_addr()?;
        Ok(Self {
            inner: Some(inner),
            local_addr,
            peer_addr: Mutex::new(None),
            monitors,
        })
    }
}

impl socket::Socket for Socket {
    fn poll_connect(&self, cx: &mut Context, peer_addr: SocketAddr) -> Poll<io::Result<()>> {
        set_current_group()?;

        let mut future = pin!(self.inner().connect(peer_addr));
        match Future::poll(future.as_mut(), cx) {
            Poll::Ready(Ok(())) => {
                *lock!(self.peer_addr) = Some(peer_addr);
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn peer_addr(&self) -> io::Result<SocketAddr> {
        if let Some(peer_addr) = *lock!(self.peer_addr) {
            Ok(peer_addr)
        } else {
            self.with_current(|| self.inner().peer_addr())
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
        if opts.source.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "setting a source address is not yet supported by the turmoil-net backend",
            ));
        }

        let destination = if destination.ip().is_unspecified() || destination.port() == 0 {
            self.peer_addr()?
        } else {
            *destination
        };

        let mut socket_write = SocketWrite {
            local_addr: &self.local_addr,
            peer_addr: &destination,
            transport: crate::net::monitor::Transport::Udp,
            payload,
            opts: &opts,
        };
        self.monitors.on_socket_write(&mut socket_write)?;

        let payload = flatten_payload(payload);
        let segments = segment_payload(&payload, opts.segment_len);
        let mut cx = cx;

        self.with_current(|| {
            let mut sent = 0;

            for segment in &segments {
                let len = if let Some(cx) = cx.as_deref_mut() {
                    let mut future = pin!(self.inner().send_to(segment, destination));
                    match Future::poll(future.as_mut(), cx) {
                        Poll::Ready(res) => res?,
                        Poll::Pending => return Err(io::ErrorKind::WouldBlock.into()),
                    }
                } else {
                    self.inner().try_send_to(segment, destination)?
                };

                sent += len;
            }

            let (monitors, packets) = registry::with_registry(|registry| {
                Ok((registry.monitors(), registry.drain_packets()))
            })?;
            let mut panic_payload = None;

            for packet in packets {
                let Some(monitor_packet) = monitor_packet(&packet) else {
                    registry::with_registry(|registry| {
                        registry.deliver(packet);
                        Ok(())
                    })?;
                    continue;
                };

                let sent = panic::catch_unwind(AssertUnwindSafe(|| {
                    monitors.on_packet_sent(&monitor_packet)
                }));
                let sent = match sent {
                    Ok(command) => command,
                    Err(payload) => {
                        panic_payload = Some(payload);
                        break;
                    }
                };

                if sent.is_drop() {
                    continue;
                }

                let received = panic::catch_unwind(AssertUnwindSafe(|| {
                    monitors.on_packet_received(&monitor_packet)
                }));
                let received = match received {
                    Ok(command) => command,
                    Err(payload) => {
                        panic_payload = Some(payload);
                        break;
                    }
                };

                if received.is_drop() {
                    continue;
                }

                registry::with_registry(|registry| {
                    registry.deliver(packet);
                    Ok(())
                })?;
            }

            if let Some(payload) = panic_payload {
                panic::resume_unwind(payload);
            }

            Ok(sent)
        })
    }

    fn recvmsg(
        &self,
        cx: Option<&mut Context>,
        payload: &mut [io::IoSliceMut],
        opts: RecvOptions,
    ) -> io::Result<RecvResult> {
        if opts.gro {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "GRO isn't supported by the turmoil-net backend",
            ));
        }

        let capacity = payload.iter().map(|chunk| chunk.len()).sum();
        let mut buffer = vec![0; capacity];

        let (received, peer_addr) = self.with_current(|| {
            if opts.peek && cx.is_none() {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "peek without a task context isn't supported by the turmoil-net backend",
                ));
            }

            if opts.peek {
                let mut future = pin!(self.inner().peek_from(&mut buffer));
                match Future::poll(future.as_mut(), cx.expect("peek requires a task context")) {
                    Poll::Ready(res) => res,
                    Poll::Pending => Err(io::ErrorKind::WouldBlock.into()),
                }
            } else if let Some(cx) = cx {
                let mut future = pin!(self.inner().recv_from(&mut buffer));
                match Future::poll(future.as_mut(), cx) {
                    Poll::Ready(res) => res,
                    Poll::Pending => Err(io::ErrorKind::WouldBlock.into()),
                }
            } else {
                self.inner().try_recv_from(&mut buffer)
            }
        })?;

        let copied = copy_payload(&buffer[..received], payload);
        let mut result = RecvResult {
            peer_addr,
            local_addr: self.local_addr,
            ecn: 0,
            len: copied,
            segment_len: copied,
            truncation_len: 0,
        };

        let mut socket_read = SocketRead {
            result: &mut result,
            payload,
        };
        self.monitors.on_socket_read(&mut socket_read)?;

        Ok(result)
    }

    fn shutdown(&self, how: std::net::Shutdown) -> io::Result<()> {
        let _ = how;
        Ok(())
    }
}

impl Socket {
    fn inner(&self) -> &TurmoilUdpSocket {
        self.inner.as_ref().expect("socket already dropped")
    }

    fn with_current<F, R>(&self, f: F) -> io::Result<R>
    where
        F: FnOnce() -> io::Result<R>,
    {
        set_current_group()?;
        f()
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        self.monitors
            .on_socket_closed(&self.local_addr, crate::net::monitor::Transport::Udp);

        let Some(inner) = self.inner.take() else {
            return;
        };

        let registry_available = registry::scope::try_borrow_with(|scope| scope.is_some());
        if !registry_available {
            std::mem::forget(inner);
            return;
        }

        let mut inner = Some(inner);
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            if set_current_group().is_ok() {
                drop(inner.take());
            }
        }));

        if let Some(inner) = inner.take() {
            std::mem::forget(inner);
        }

        let _ = result;
    }
}

fn set_current_group() -> io::Result<()> {
    let group = crate::group::current();
    registry::with_registry(|registry| registry.set_current_group(&group))
}

fn flatten_payload(payload: &[io::IoSlice]) -> Vec<u8> {
    let len = payload.iter().map(|chunk| chunk.len()).sum();
    let mut out = Vec::with_capacity(len);
    for chunk in payload {
        out.extend_from_slice(chunk);
    }
    out
}

fn segment_payload(payload: &[u8], segment_len: Option<usize>) -> Vec<Vec<u8>> {
    match segment_len {
        Some(0) => vec![payload.to_vec()],
        Some(segment_len) => payload.chunks(segment_len).map(ToOwned::to_owned).collect(),
        None => vec![payload.to_vec()],
    }
}

fn copy_payload(src: &[u8], dst: &mut [io::IoSliceMut]) -> usize {
    let mut copied = 0;
    let mut remaining = src;

    for chunk in dst {
        let len = chunk.len().min(remaining.len());
        chunk[..len].copy_from_slice(&remaining[..len]);
        copied += len;
        remaining = &remaining[len..];

        if remaining.is_empty() {
            break;
        }
    }

    copied
}

fn monitor_packet(packet: &turmoil_net::Packet) -> Option<crate::environment::net::ip::Packet> {
    registry::monitor_packet(packet)
}
