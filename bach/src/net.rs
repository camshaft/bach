use std::io;

mod addr;
pub mod socket;
mod udp;

#[cfg(feature = "net-monitor")]
pub mod monitor;
#[cfg(not(feature = "net-monitor"))]
#[allow(dead_code)]
pub(crate) mod monitor;

pub use addr::ToSocketAddrs;
pub use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, SocketAddrV4, SocketAddrV6};
pub use udp::UdpSocket;

pub async fn lookup_host<T>(host: T) -> io::Result<impl Iterator<Item = SocketAddr>>
where
    T: ToSocketAddrs,
{
    Ok(core::iter::once(addr::lookup_host(host)?))
}
