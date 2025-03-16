use crate::environment::net::pcap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};

pub mod header;
pub mod segments;
pub mod transport;

pub use header::{Header, Version};
pub use segments::Segments;
pub use transport::Transport;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Packet {
    pub header: Header,
    pub transport: Transport,
}

impl Packet {
    pub fn source(&self) -> SocketAddr {
        let ip = self.header.source();
        let port = self.transport.source();
        SocketAddr::new(ip, port)
    }

    pub fn destination(&self) -> SocketAddr {
        let ip = self.header.destination();
        let port = self.transport.destination();
        SocketAddr::new(ip, port)
    }

    pub fn update_checksum(&mut self) {
        self.transport.update_checksum(&self.header)
    }
}

impl pcap::AsPcap for Packet {
    fn as_pcap<W: std::io::Write>(&self, out: &mut W) -> std::io::Result<()> {
        // write an empty ethernet addresses
        out.write_all(&[0; 6 * 2])?;

        self.header.as_pcap(out, &self.transport)?;
        self.transport.as_pcap(out)
    }
}

pub trait Category {
    fn is_unspecified(&self) -> bool;
    fn is_loopback(&self) -> bool;
}

impl Category for Ipv4Addr {
    fn is_unspecified(&self) -> bool {
        self.is_unspecified()
    }

    fn is_loopback(&self) -> bool {
        self.is_loopback()
    }
}

impl Category for Ipv6Addr {
    fn is_unspecified(&self) -> bool {
        self.is_unspecified()
    }

    fn is_loopback(&self) -> bool {
        self.is_loopback()
    }
}

impl Category for IpAddr {
    fn is_unspecified(&self) -> bool {
        match self {
            IpAddr::V4(addr) => addr.is_unspecified(),
            IpAddr::V6(addr) => addr.is_unspecified(),
        }
    }

    fn is_loopback(&self) -> bool {
        match self {
            IpAddr::V4(addr) => addr.is_loopback(),
            IpAddr::V6(addr) => addr.is_loopback(),
        }
    }
}

impl Category for SocketAddrV4 {
    fn is_unspecified(&self) -> bool {
        self.ip().is_unspecified() || self.port() == 0
    }

    fn is_loopback(&self) -> bool {
        self.ip().is_loopback()
    }
}

impl Category for SocketAddrV6 {
    fn is_unspecified(&self) -> bool {
        self.ip().is_unspecified() || self.port() == 0
    }

    fn is_loopback(&self) -> bool {
        self.ip().is_loopback()
    }
}

impl Category for SocketAddr {
    fn is_unspecified(&self) -> bool {
        match self {
            SocketAddr::V4(addr) => addr.is_unspecified(),
            SocketAddr::V6(addr) => addr.is_unspecified(),
        }
    }

    fn is_loopback(&self) -> bool {
        match self {
            SocketAddr::V4(addr) => addr.is_loopback(),
            SocketAddr::V6(addr) => addr.is_loopback(),
        }
    }
}

pub enum Allocator {
    V4(u32),
    V6(u128),
}

impl Default for Allocator {
    fn default() -> Self {
        Self::new(IpAddr::V4([10, 0, 0, 1].into()))
    }
}

impl Allocator {
    pub fn new(start: IpAddr) -> Self {
        match start {
            IpAddr::V4(addr) => Self::V4(addr.into()),
            IpAddr::V6(addr) => Self::V6(addr.into()),
        }
    }

    pub fn localhost(&self) -> IpAddr {
        match self {
            Self::V4(_) => Ipv4Addr::LOCALHOST.into(),
            Self::V6(_) => Ipv6Addr::LOCALHOST.into(),
        }
    }

    pub fn allocate(&mut self) -> IpAddr {
        match self {
            Self::V4(ref mut ip) => {
                let v = *ip;
                *ip += 1;
                v.to_be_bytes().into()
            }
            Self::V6(ref mut ip) => {
                let v = *ip;
                *ip += 1;
                v.to_be_bytes().into()
            }
        }
    }
}
