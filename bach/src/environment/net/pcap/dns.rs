use super::{block::NameResolution, AsPcap};
use std::{io, net::IpAddr};

pub fn write<W>(out: &mut W, query: &str, result: &IpAddr) -> io::Result<()>
where
    W: io::Write,
{
    NameResolution {
        addr: *result,
        name: query,
    }
    .as_pcap(out)?;

    Ok(())
}
