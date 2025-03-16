use crate::environment::net::ip::{header, transport::Udp, Packet};
use bytes::Bytes;
use std::{
    io,
    net::{IpAddr, SocketAddr},
};

use super::AsPcap;

pub fn write<W>(out: &mut W, addr: &SocketAddr, query: &str, result: &IpAddr) -> io::Result<()>
where
    W: io::Write,
{
    // We just need to write the response - wireshark will still accept it as a valid response and fill in
    // all of the host names.
    Packet {
        header: match addr {
            SocketAddr::V4(addr) => header::V4 {
                source: *addr.ip(),
                destination: *addr.ip(),
                ..Default::default()
            }
            .into(),
            SocketAddr::V6(addr) => header::V6 {
                source: *addr.ip(),
                destination: *addr.ip(),
                ..Default::default()
            }
            .into(),
        },
        transport: Udp {
            source: addr.port(),
            destination: addr.port(),
            payload: Response { query, result }.into(),
            checksum: 0,
        }
        .into(),
    }
    .as_pcap_record()
    .as_pcap(out)?;

    Ok(())
}

struct Response<'a> {
    query: &'a str,
    result: &'a IpAddr,
}

impl AsPcap for Response<'_> {
    fn as_pcap<W>(&self, out: &mut W) -> io::Result<()>
    where
        W: io::Write,
    {
        let transaction_id = 0u16;
        out.write_all(&transaction_id.to_be_bytes())?;

        let flags = 0x8120u16;
        out.write_all(&flags.to_be_bytes())?;

        let questions = 1u16;
        out.write_all(&questions.to_be_bytes())?;

        let answers = 1u16;
        out.write_all(&answers.to_be_bytes())?;

        let authority = 0u16;
        out.write_all(&authority.to_be_bytes())?;

        let additional = 0u16;
        out.write_all(&additional.to_be_bytes())?;

        write_query(out, self.query, self.result)?;

        let name = 0xc00cu16;
        out.write_all(&name.to_be_bytes())?;
        let ty = match self.result {
            IpAddr::V4(_) => 1u16,
            IpAddr::V6(_) => 28u16,
        };
        out.write_all(&ty.to_be_bytes())?;
        let class_in = 1u16;
        out.write_all(&class_in.to_be_bytes())?;
        let ttl = u16::MAX as u32;
        out.write_all(&ttl.to_be_bytes())?;

        macro_rules! write_ip {
            ($value:expr) => {{
                let value = $value.octets();
                let length = value.len() as u16;
                out.write_all(&length.to_be_bytes())?;
                out.write_all(&value)?
            }};
        }

        match self.result {
            IpAddr::V4(ip) => write_ip!(ip),
            IpAddr::V6(ip) => write_ip!(ip),
        };

        Ok(())
    }
}

impl From<Response<'_>> for Bytes {
    fn from(val: Response<'_>) -> Self {
        let mut out = vec![];
        val.as_pcap(&mut out).unwrap();
        out.into()
    }
}

fn write_query(out: &mut impl io::Write, query: &str, result: &IpAddr) -> io::Result<()> {
    for part in query.split('.') {
        let length = part.len() as u8;
        out.write_all(&[length])?;
        out.write_all(part.as_bytes())?;
    }
    out.write_all(&[0u8])?;

    match result {
        IpAddr::V4(_) => {
            let type_a = 1u16;
            out.write_all(&type_a.to_be_bytes())?;
            let class_in = 1u16;
            out.write_all(&class_in.to_be_bytes())?;
        }
        IpAddr::V6(_) => {
            let type_aaaa = 28u16;
            out.write_all(&type_aaaa.to_be_bytes())?;
            let class_in = 1u16;
            out.write_all(&class_in.to_be_bytes())?;
        }
    }

    Ok(())
}
