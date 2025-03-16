use super::UdpSocket;
use crate::net::{Ipv4Addr, SocketAddr};
use std::io;

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Options {
    pub local_addr: SocketAddr,
    pub no_delay: bool,
    pub reuse_port: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            local_addr: (Ipv4Addr::UNSPECIFIED, 0).into(),
            no_delay: false,
            reuse_port: false,
        }
    }
}

impl Options {
    pub fn build_udp(&self) -> io::Result<UdpSocket> {
        UdpSocket::new(self)
    }
}

#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct SendOptions {
    pub source: Option<SocketAddr>,
    pub ecn: u8,
    pub segment_len: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct RecvOptions {
    pub peek: bool,
    pub gro: bool,
}

#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct RecvResult {
    pub peer_addr: SocketAddr,
    pub local_addr: SocketAddr,
    pub ecn: u8,
    pub len: usize,
    pub segment_len: usize,
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum Sockopt {
    Delay,
    Tos,
    Ttl,
    SendBuffer,
    ReceiveBuffer,
}
