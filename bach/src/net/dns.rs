use super::SocketAddr;
use std::io;

pub async fn lookup_host<T>(host: T) -> io::Result<impl Iterator<Item = SocketAddr>>
where
    T: ToSocketAddrs,
{
    // host.to_socket_addrs(sealed::Internal).await
    Ok([].into_iter())
}

pub trait ToSocketAddrs {}

impl<T: ToSocketAddrs + ?Sized> ToSocketAddrs for &T {}
