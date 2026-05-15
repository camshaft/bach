use super::Transport;
use crate::{
    environment::net::pcap::AsPcap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};
use std::io;

#[derive(Clone, Debug, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Version {
    V4,
    V6,
}

#[derive(Clone, Debug, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Header {
    V4(V4),
    V6(V6),
}

impl Header {
    pub fn version(&self) -> Version {
        match self {
            Header::V4 { .. } => Version::V4,
            Header::V6 { .. } => Version::V6,
        }
    }

    pub fn source(&self) -> IpAddr {
        match self {
            Header::V4(h) => IpAddr::V4(h.source),
            Header::V6(h) => IpAddr::V6(h.source),
        }
    }

    pub fn destination(&self) -> IpAddr {
        match self {
            Header::V4(h) => IpAddr::V4(h.destination),
            Header::V6(h) => IpAddr::V6(h.destination),
        }
    }

    pub fn ecn(&self) -> u8 {
        match self {
            Header::V4(h) => h.ecn(),
            Header::V6(h) => h.ecn(),
        }
    }

    pub fn set_ecn(&mut self, ecn: u8) {
        match self {
            Header::V4(h) => h.set_ecn(ecn),
            Header::V6(h) => h.set_ecn(ecn),
        }
    }

    pub(super) fn as_pcap<O: io::Write>(
        &self,
        out: &mut O,
        transport: &Transport,
    ) -> io::Result<()> {
        // https://www.iana.org/assignments/ieee-802-numbers/ieee-802-numbers.xhtml#ieee-802-numbers-1
        match self {
            Header::V4(h) => {
                // write the EtherType for IPv4
                out.write_all(&0x0800u16.to_be_bytes())?;
                h.as_pcap(out, transport)
            }
            Header::V6(h) => {
                // write the EtherType for IPv4
                out.write_all(&0x86DDu16.to_be_bytes())?;
                h.as_pcap(out, transport)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct V4 {
    pub source: Ipv4Addr,
    pub destination: Ipv4Addr,
    pub dscp: u8,
    pub ecn: u8,
    pub id: u16,
    pub df: bool,
    pub ttl: u8,
}

impl Default for V4 {
    fn default() -> Self {
        Self {
            source: Ipv4Addr::UNSPECIFIED,
            destination: Ipv4Addr::UNSPECIFIED,
            dscp: 0,
            ecn: 0,
            id: 0,
            df: true,
            ttl: 64,
        }
    }
}

impl V4 {
    pub fn ecn(&self) -> u8 {
        self.ecn
    }

    pub fn set_ecn(&mut self, ecn: u8) {
        self.ecn = ecn;
    }

    fn as_pcap<O: io::Write>(&self, out: &mut O, transport: &Transport) -> io::Result<()> {
        const HEADER_LEN: usize = 20;

        let mut buffer = [0u8; HEADER_LEN];

        let transport_len = transport.pcap_len()?;

        let total_len: u16 = (transport_len + HEADER_LEN).try_into().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "total length too large for IPv4 payload",
            )
        })?;

        // version (4) + IHL (5 = 20 bytes)
        buffer[0] = (4 << 4) | 5;
        // DSCP (6 bits) + ECN (2 bits)
        buffer[1] = ((self.dscp & 0x3F) << 2) | (self.ecn & 0x03);
        // total length
        buffer[2..4].copy_from_slice(&total_len.to_be_bytes());
        // identification
        buffer[4..6].copy_from_slice(&self.id.to_be_bytes());
        // flags (3 bits) + fragment offset (13 bits)
        let flags: u16 = if self.df { 0x4000 } else { 0 };
        buffer[6..8].copy_from_slice(&flags.to_be_bytes());
        // TTL
        buffer[8] = self.ttl;
        // protocol
        buffer[9] = transport.protocol();
        // checksum (zero for initial calculation)
        buffer[10..12].copy_from_slice(&[0, 0]);
        // source address
        buffer[12..16].copy_from_slice(&self.source.octets());
        // destination address
        buffer[16..20].copy_from_slice(&self.destination.octets());

        // calculate IPv4 header checksum
        let mut sum: u32 = 0;
        for i in (0..HEADER_LEN).step_by(2) {
            sum += u16::from_be_bytes([buffer[i], buffer[i + 1]]) as u32;
        }
        while sum >> 16 != 0 {
            sum = (sum & 0xFFFF) + (sum >> 16);
        }
        let checksum = !(sum as u16);
        buffer[10..12].copy_from_slice(&checksum.to_be_bytes());

        out.write_all(&buffer)?;

        Ok(())
    }
}

impl From<V4> for Header {
    fn from(h: V4) -> Self {
        Header::V4(h)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct V6 {
    pub source: Ipv6Addr,
    pub destination: Ipv6Addr,
    pub dscp: u8,
    pub ecn: u8,
    pub flow_label: u32,
    pub hop_limit: u8,
}

impl Default for V6 {
    fn default() -> Self {
        Self {
            source: Ipv6Addr::UNSPECIFIED,
            destination: Ipv6Addr::UNSPECIFIED,
            dscp: 0,
            ecn: 0,
            flow_label: 0,
            hop_limit: 64,
        }
    }
}

impl V6 {
    pub fn ecn(&self) -> u8 {
        self.ecn
    }

    pub fn set_ecn(&mut self, ecn: u8) {
        self.ecn = ecn;
    }

    fn as_pcap<O: io::Write>(&self, out: &mut O, transport: &Transport) -> io::Result<()> {
        const HEADER_LEN: usize = 40;

        let mut buffer = [0u8; HEADER_LEN];

        let payload_len: u16 = transport.pcap_len()?.try_into().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "total length too large for IPv6 payload",
            )
        })?;

        // version (4 bits) + traffic class (8 bits: DSCP 6 + ECN 2) + flow label (20 bits)
        let tc = ((self.dscp as u32 & 0x3F) << 2) | (self.ecn as u32 & 0x03);
        let vtcfl: u32 = (6 << 28) | (tc << 20) | (self.flow_label & 0x000F_FFFF);
        buffer[0..4].copy_from_slice(&vtcfl.to_be_bytes());
        // payload length
        buffer[4..6].copy_from_slice(&payload_len.to_be_bytes());
        // next header (protocol)
        buffer[6] = transport.protocol();
        // hop limit
        buffer[7] = self.hop_limit;
        // source address (16 bytes)
        buffer[8..24].copy_from_slice(&self.source.octets());
        // destination address (16 bytes)
        buffer[24..40].copy_from_slice(&self.destination.octets());

        out.write_all(&buffer)?;

        Ok(())
    }
}

impl From<V6> for Header {
    fn from(h: V6) -> Self {
        Header::V6(h)
    }
}
