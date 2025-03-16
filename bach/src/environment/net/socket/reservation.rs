use crate::net::SocketAddr;
use core::task::{Context, Poll};
use std::io;

pub struct Socket<S, R> {
    inner: S,
    #[allow(dead_code)]
    reservation: R,
}

impl<S, R> Socket<S, R> {
    pub fn new(inner: S, reservation: R) -> Self {
        Self { inner, reservation }
    }
}

impl<S, R> super::Socket for Socket<S, R>
where
    S: super::Socket,
    R: 'static + Send + Sync,
{
    fn poll_connect(&self, cx: &mut Context, peer_addr: SocketAddr) -> Poll<io::Result<()>> {
        self.inner.poll_connect(cx, peer_addr)
    }

    fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.inner.peer_addr()
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    fn poll_writable(&self, cx: &mut Context) -> Poll<io::Result<()>> {
        self.inner.poll_writable(cx)
    }

    fn poll_readable(&self, cx: &mut Context) -> Poll<io::Result<()>> {
        self.inner.poll_readable(cx)
    }

    fn set_sockopt(&self, option: super::Sockopt, value: usize) -> io::Result<()> {
        self.inner.set_sockopt(option, value)
    }

    fn get_sockopt(&self, option: super::Sockopt) -> io::Result<usize> {
        self.inner.get_sockopt(option)
    }

    fn sendmsg(
        &self,
        cx: Option<&mut Context>,
        destination: &SocketAddr,
        payload: &[io::IoSlice],
        opts: super::SendOptions,
    ) -> io::Result<usize> {
        self.inner.sendmsg(cx, destination, payload, opts)
    }

    fn recvmsg(
        &self,
        cx: Option<&mut Context>,
        payload: &mut [io::IoSliceMut],
        opts: super::RecvOptions,
    ) -> io::Result<super::RecvResult> {
        self.inner.recvmsg(cx, payload, opts)
    }

    fn shutdown(&self, how: std::net::Shutdown) -> io::Result<()> {
        self.inner.shutdown(how)
    }
}
