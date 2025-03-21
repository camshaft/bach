use crate::{
    environment::net::{
        registry,
        socket::{self, Socket},
    },
    net::{
        addr::{self, ToSocketAddrs},
        SocketAddr,
    },
};
use std::{
    future::poll_fn,
    io::{IoSlice, IoSliceMut, Result},
    task::{ready, Context, Poll},
};
use tokio::io::ReadBuf;

pub struct UdpSocket(Box<dyn Socket>);

impl UdpSocket {
    pub fn new(options: &socket::Options) -> Result<Self> {
        let group = crate::group::current();
        let socket = registry::with_registry(|r| r.register_udp_socket(&group, options))?;
        Ok(Self(socket))
    }

    pub async fn bind<A: ToSocketAddrs>(addr: A) -> Result<Self> {
        let local_addr = addr::lookup_host(addr)?;

        let opts = socket::Options {
            local_addr,
            ..Default::default()
        };
        let socket = Self::new(&opts)?;

        // yield the task in case multiple sockets are being bound in this tick
        crate::task::yield_now().await;

        Ok(socket)
    }

    pub async fn connect<A: ToSocketAddrs>(&self, addr: A) -> Result<()> {
        let addr = addr::lookup_host(addr)?;
        poll_fn(|cx| self.0.poll_connect(cx, addr)).await
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.0.local_addr()
    }

    pub fn peer_addr(&self) -> Result<SocketAddr> {
        self.0.peer_addr()
    }

    pub async fn send(&self, buf: &[u8]) -> Result<usize> {
        let addr = self.peer_addr()?;
        self.send_to(buf, addr).await
    }

    pub fn poll_send(&self, cx: &mut Context, buf: &[u8]) -> Poll<Result<usize>> {
        let addr = self.peer_addr()?;
        self.poll_send_to(cx, buf, addr)
    }

    pub fn try_send(&self, buf: &[u8]) -> Result<usize> {
        let target = self.peer_addr()?;
        self.try_send_to(buf, target)
    }

    pub async fn send_to<A: ToSocketAddrs>(&self, buf: &[u8], target: A) -> Result<usize> {
        let addr = addr::lookup_host(target)?;
        poll_fn(|cx| self.poll_send_to(cx, buf, addr)).await
    }

    pub fn try_send_to<A: ToSocketAddrs>(&self, buf: &[u8], target: A) -> Result<usize> {
        let addr = addr::lookup_host(target)?;
        let buf = &[IoSlice::new(buf)];
        self.0.sendmsg(None, &addr, buf, Default::default())
    }

    pub fn poll_send_to<A: ToSocketAddrs>(
        &self,
        cx: &mut Context,
        buf: &[u8],
        addr: A,
    ) -> Poll<Result<usize>> {
        self.poll_send_msg(cx, addr, &[IoSlice::new(buf)], Default::default())
    }

    pub async fn send_msg<A: ToSocketAddrs>(
        &self,
        addr: A,
        buf: &[IoSlice<'_>],
        opts: socket::SendOptions,
    ) -> Result<usize> {
        let addr = addr::lookup_host(addr)?;
        poll_fn(|cx| self.poll_send_msg(cx, addr, buf, opts)).await
    }

    pub fn poll_send_msg<A: ToSocketAddrs>(
        &self,
        cx: &mut Context,
        addr: A,
        buf: &[IoSlice],
        opts: socket::SendOptions,
    ) -> Poll<Result<usize>> {
        let addr = addr::lookup_host(addr)?;
        match self.0.sendmsg(Some(cx), &addr, buf, opts) {
            Ok(len) => Poll::Ready(Ok(len)),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Poll::Pending,
            Err(e) => Poll::Ready(Err(e)),
        }
    }

    pub fn try_send_msg<A: ToSocketAddrs>(
        &self,
        addr: A,
        buf: &[IoSlice],
        opts: socket::SendOptions,
    ) -> Result<usize> {
        let addr = addr::lookup_host(addr)?;
        self.0.sendmsg(None, &addr, buf, opts)
    }

    pub async fn writable(&self) -> Result<()> {
        poll_fn(|cx| self.0.poll_writable(cx)).await
    }

    pub fn poll_send_ready(&self, cx: &mut Context) -> Poll<Result<()>> {
        self.0.poll_writable(cx)
    }

    pub async fn recv(&self, buf: &mut [u8]) -> Result<usize> {
        let (len, _peer) = self.recv_from(buf).await?;
        Ok(len)
    }

    pub fn poll_recv(&self, cx: &mut Context, buf: &mut ReadBuf) -> Poll<Result<()>> {
        ready!(self.poll_recv_from(cx, buf))?;
        Ok(()).into()
    }

    pub fn try_recv(&self, buf: &mut [u8]) -> Result<usize> {
        let (len, _peer) = self.try_recv_from(buf)?;
        Ok(len)
    }

    pub async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        poll_fn(|cx| {
            let mut buf = ReadBuf::new(buf);
            let addr = ready!(self.poll_recv_from(cx, &mut buf))?;
            Ok((buf.filled().len(), addr)).into()
        })
        .await
    }

    pub fn try_recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        let buf = &mut [IoSliceMut::new(buf)];
        let res = self.0.recvmsg(None, buf, Default::default())?;
        Ok((res.len, res.peer_addr))
    }

    pub fn poll_recv_from(&self, cx: &mut Context, buf: &mut ReadBuf) -> Poll<Result<SocketAddr>> {
        let payload = &mut [IoSliceMut::new(buf.initialize_unfilled())];
        let res = ready!(self.poll_recv_msg(cx, payload, Default::default()))?;
        buf.advance(res.len);
        Ok(res.peer_addr).into()
    }

    pub fn poll_recv_msg(
        &self,
        cx: &mut Context,
        payload: &mut [IoSliceMut],
        opts: socket::RecvOptions,
    ) -> Poll<Result<socket::RecvResult>> {
        match self.0.recvmsg(Some(cx), payload, opts) {
            Ok(res) => Poll::Ready(Ok(res)),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Poll::Pending,
            Err(e) => Poll::Ready(Err(e)),
        }
    }

    pub async fn readable(&self) -> Result<()> {
        poll_fn(|cx| self.0.poll_readable(cx)).await
    }

    pub fn poll_recv_ready(&self, cx: &mut Context) -> Poll<Result<()>> {
        self.0.poll_readable(cx)
    }
}
