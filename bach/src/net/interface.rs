use super::SocketAddr;
use std::{
    io,
    sync::Arc,
    task::{Context, Poll},
};

crate::scope::define!(scope, Box<dyn Interface>);

pub enum Protocol {
    Tcp,
    Udp,
}

pub trait Interface {
    fn register_socket(&self, addr: SocketAddr, protocol: Protocol) -> Arc<dyn Socket>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tos(pub u32);

pub trait Socket {
    fn local_addr(&self) -> io::Result<SocketAddr>;
    fn peer_addr(&self) -> io::Result<SocketAddr>;
    fn poll_send(
        &self,
        cx: &mut Context,
        buf: &io::IoSlice,
        destination: Option<SocketAddr>,
        tos: Option<Tos>,
    ) -> Poll<io::Result<usize>>;
    fn poll_recv(
        &self,
        cx: &mut Context,
        buf: &mut io::IoSliceMut,
    ) -> Poll<io::Result<(usize, SocketAddr, Tos)>>;
}
