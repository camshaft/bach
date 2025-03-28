use crate::environment::net::pcap::AsPcap;
use bytes::Bytes;
use s2n_quic_core::inet::Protocol;
use std::io::{self, IoSliceMut};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Kind {
    Tcp,
    Udp,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Transport {
    Tcp(Tcp),
    Udp(Udp),
}

impl Transport {
    pub fn kind(&self) -> Kind {
        match self {
            Transport::Tcp(_) => Kind::Tcp,
            Transport::Udp(_) => Kind::Udp,
        }
    }

    pub fn source(&self) -> u16 {
        match self {
            Transport::Tcp(t) => t.source,
            Transport::Udp(t) => t.source,
        }
    }

    pub fn destination(&self) -> u16 {
        match self {
            Transport::Tcp(t) => t.destination,
            Transport::Udp(t) => t.destination,
        }
    }

    pub fn payload(&self) -> &Bytes {
        match self {
            Transport::Tcp(t) => &t.payload,
            Transport::Udp(t) => &t.payload,
        }
    }

    pub fn payload_mut(&mut self) -> &mut Bytes {
        match self {
            Transport::Tcp(t) => &mut t.payload,
            Transport::Udp(t) => &mut t.payload,
        }
    }

    pub fn protocol(&self) -> Protocol {
        match self {
            Transport::Tcp(_) => Protocol::TCP,
            Transport::Udp(_) => Protocol::UDP,
        }
    }

    pub fn update_checksum(&mut self, header: &super::Header) {
        match self {
            Transport::Udp(t) => t.update_checksum(header),
            Transport::Tcp(t) => t.update_checksum(header),
        }
    }

    /// Copies the payload into the destination buffer
    ///
    /// Returns:
    /// 0: the number of bytes copied
    /// 1: the number of bytes still in the payload - this can be used to detect truncation
    pub fn copy_payload_into(&self, chunks: &mut [IoSliceMut]) -> (usize, usize) {
        let mut copied_len = 0;
        let mut p = &self.payload()[..];
        for chunk in chunks {
            let n = chunk.len().min(p.len());
            copied_len += n;

            let (src, remaining) = p.split_at(n);
            chunk[..n].copy_from_slice(src);

            p = remaining;

            if remaining.is_empty() {
                break;
            }
        }

        let remaining_len = p.len();

        (copied_len, remaining_len)
    }
}

impl AsPcap for Transport {
    fn as_pcap<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        match self {
            Transport::Tcp(transport) => transport.as_pcap(out),
            Transport::Udp(transport) => transport.as_pcap(out),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tcp {
    pub source: u16,
    pub destination: u16,
    pub payload: Bytes,
}

impl Tcp {
    pub fn update_checksum(&mut self, _header: &super::Header) {
        todo!()
    }
}

impl AsPcap for Tcp {
    fn as_pcap<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        let _ = out;
        todo!()
    }
}

impl From<Tcp> for Transport {
    fn from(value: Tcp) -> Self {
        Transport::Tcp(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Udp {
    pub source: u16,
    pub destination: u16,
    pub payload: Bytes,
    pub checksum: u16,
}

impl Udp {
    pub fn update_checksum(&mut self, header: &super::Header) {
        // IPv4 doesn't require checksums

        if let super::Header::V6(header) = header {
            // TODO
            let _ = header;
        }
    }
}

impl AsPcap for Udp {
    fn as_pcap<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        //= https://www.rfc-editor.org/rfc/rfc768.html#name-format
        //# 0      7 8     15 16    23 24    31
        //# +--------+--------+--------+--------+
        //# |     Source      |   Destination   |
        //# |      Port       |      Port       |
        //# +--------+--------+--------+--------+
        //# |                 |                 |
        //# |     Length      |    Checksum     |
        //# +--------+--------+--------+--------+

        out.write_all(&self.source.to_be_bytes())?;
        out.write_all(&self.destination.to_be_bytes())?;
        let len: u16 = (self.payload.len() + 8).try_into().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "payload too large for UDP packet",
            )
        })?;
        out.write_all(&len.to_be_bytes())?;
        out.write_all(&self.checksum.to_be_bytes())?;

        out.write_all(&self.payload)?;

        Ok(())
    }
}

impl From<Udp> for Transport {
    fn from(value: Udp) -> Self {
        Transport::Udp(value)
    }
}
