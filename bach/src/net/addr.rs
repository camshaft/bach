use crate::{
    environment::net::registry,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
};
use std::io;

pub(crate) fn lookup_host<Addr: ToSocketAddrs>(v: Addr) -> io::Result<SocketAddr> {
    v.to_socket_addr(sealed::internal())
}

pub trait ToIpAddr: sealed::Sealed {
    #[doc(hidden)]
    fn to_ip_addr(&self, internal: sealed::Internal) -> io::Result<IpAddr>;
}

impl ToIpAddr for String {
    fn to_ip_addr(&self, internal: sealed::Internal) -> io::Result<IpAddr> {
        (&self[..]).to_ip_addr(internal)
    }
}

impl ToIpAddr for &str {
    fn to_ip_addr(&self, _internal: sealed::Internal) -> io::Result<IpAddr> {
        if let Ok(ipaddr) = self.parse() {
            return Ok(ipaddr);
        }

        let group = crate::group::current();
        registry::with_registry(|r| r.resolve_host(&group, self))
    }
}

impl ToIpAddr for IpAddr {
    fn to_ip_addr(&self, _internal: sealed::Internal) -> io::Result<IpAddr> {
        Ok(*self)
    }
}

impl ToIpAddr for Ipv4Addr {
    fn to_ip_addr(&self, _internal: sealed::Internal) -> io::Result<IpAddr> {
        Ok((*self).into())
    }
}

impl ToIpAddr for Ipv6Addr {
    fn to_ip_addr(&self, _internal: sealed::Internal) -> io::Result<IpAddr> {
        Ok((*self).into())
    }
}

pub trait ToSocketAddrs: sealed::Sealed {
    fn to_socket_addr(&self, internal: sealed::Internal) -> io::Result<SocketAddr>;
}

impl ToSocketAddrs for SocketAddr {
    fn to_socket_addr(&self, _internal: sealed::Internal) -> io::Result<SocketAddr> {
        Ok(*self)
    }
}

impl ToSocketAddrs for SocketAddrV4 {
    fn to_socket_addr(&self, _internal: sealed::Internal) -> io::Result<SocketAddr> {
        Ok((*self).into())
    }
}

impl ToSocketAddrs for SocketAddrV6 {
    fn to_socket_addr(&self, _internal: sealed::Internal) -> io::Result<SocketAddr> {
        Ok((*self).into())
    }
}

impl<Ip: ToIpAddr> ToSocketAddrs for (Ip, u16) {
    fn to_socket_addr(&self, internal: sealed::Internal) -> io::Result<SocketAddr> {
        let ip = self.0.to_ip_addr(internal)?;
        Ok((ip, self.1).into())
    }
}

impl<T: ToSocketAddrs + ?Sized> ToSocketAddrs for &T {
    fn to_socket_addr(&self, internal: sealed::Internal) -> io::Result<SocketAddr> {
        (**self).to_socket_addr(internal)
    }
}

impl ToSocketAddrs for str {
    fn to_socket_addr(&self, internal: sealed::Internal) -> io::Result<SocketAddr> {
        let socketaddr: Result<SocketAddr, _> = self.parse();

        if let Ok(s) = socketaddr {
            return Ok(s);
        }

        macro_rules! try_opt {
            ($e:expr, $msg:expr) => {
                match $e {
                    Some(r) => r,
                    None => return Err(io::Error::new(io::ErrorKind::InvalidInput, $msg)),
                }
            };
        }

        // split the string by ':' and convert the second part to u16
        let (host, port_str) = try_opt!(self.rsplit_once(':'), "invalid socket address");
        let port: u16 = try_opt!(port_str.parse().ok(), "invalid port value");

        (host, port).to_socket_addr(internal)
    }
}

impl ToSocketAddrs for String {
    fn to_socket_addr(&self, internal: sealed::Internal) -> io::Result<SocketAddr> {
        self.as_str().to_socket_addr(internal)
    }
}

mod sealed {
    pub trait Sealed {}

    impl<T: ?Sized> Sealed for T {}

    pub struct Internal(());

    pub(crate) fn internal() -> Internal {
        Internal(())
    }
}
