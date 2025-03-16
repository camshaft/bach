pub use crate::net::socket::*;
use std::{
    io::{IoSlice, IoSliceMut, Result},
    net::{Shutdown, SocketAddr},
    task::{Context, Poll},
};

pub mod reservation;
pub mod udp;

pub trait Socket: 'static + Send + Sync {
    fn poll_connect(&self, cx: &mut Context, peer_addr: SocketAddr) -> Poll<Result<()>>;

    fn peer_addr(&self) -> Result<SocketAddr>;

    fn local_addr(&self) -> Result<SocketAddr>;

    fn poll_writable(&self, cx: &mut Context) -> Poll<Result<()>>;

    fn poll_readable(&self, cx: &mut Context) -> Poll<Result<()>>;

    fn set_sockopt(&self, option: Sockopt, value: usize) -> Result<()>;

    fn get_sockopt(&self, option: Sockopt) -> Result<usize>;

    fn sendmsg(
        &self,
        cx: Option<&mut Context>,
        destination: &SocketAddr,
        payload: &[IoSlice],
        opts: SendOptions,
    ) -> Result<usize>;

    fn recvmsg(
        &self,
        cx: Option<&mut Context>,
        payload: &mut [IoSliceMut],
        opts: RecvOptions,
    ) -> Result<RecvResult>;

    fn shutdown(&self, how: Shutdown) -> Result<()>;

    // TODO maybe add `libc::msghdr` methods?
}
