use super::Transport;
use crate::{
    environment::net::pcap::AsPcap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};
use core::mem::size_of;
use s2n_quic_core::{
    havoc::{Encoder, EncoderBuffer},
    inet::{ipv4, ipv6, ExplicitCongestionNotification},
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
        const HEADER_LEN: usize = size_of::<ipv4::Header>();

        let mut buffer = [0u8; HEADER_LEN];

        let transport_len = transport.pcap_len()?;

        let total_len: u16 = (transport_len + HEADER_LEN).try_into().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "total length too large for IPv4 payload",
            )
        })?;

        {
            let mut buffer = EncoderBuffer::new(&mut buffer);
            buffer.write_zerocopy(|header: &mut ipv4::Header| {
                header.vihl_mut().set_version(4).set_header_len(5);
                header
                    .tos_mut()
                    .set_dscp(self.dscp)
                    .set_ecn(ExplicitCongestionNotification::new(self.ecn));
                header
                    .flag_fragment_mut()
                    .set_reserved(false)
                    .set_dont_fragment(self.df)
                    .set_more_fragments(false)
                    .set_fragment_offset(0);
                header.id_mut().set(self.id);
                header.total_len_mut().set(total_len);
                *header.ttl_mut() = self.ttl;
                // set the checksum to zero for the initial pass
                header.checksum_mut().set(0);
                *header.protocol_mut() = transport.protocol();
                *header.source_mut() = self.source.octets().into();
                *header.destination_mut() = self.destination.octets().into();

                // calculate the IPv4 header checksum
                header.update_checksum();
            });
        }

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
        const HEADER_LEN: usize = size_of::<ipv6::Header>();

        let mut buffer = [0u8; HEADER_LEN];

        let payload_len = transport.pcap_len()?.try_into().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "total length too large for IPv6 payload",
            )
        })?;

        {
            let mut buffer = EncoderBuffer::new(&mut buffer);
            buffer.write_zerocopy(|header: &mut ipv6::Header| {
                header
                    .vtcfl_mut()
                    .set_version(6)
                    .set_dscp(self.dscp)
                    .set_ecn(ExplicitCongestionNotification::new(self.ecn))
                    .set_flow_label(self.flow_label);
                header.payload_len_mut().set(payload_len);
                *header.next_header_mut() = transport.protocol();
                *header.hop_limit_mut() = self.hop_limit;
                *header.source_mut() = self.source.octets().into();
                *header.destination_mut() = self.destination.octets().into();
            });
        }

        out.write_all(&buffer)?;

        Ok(())
    }
}

impl From<V6> for Header {
    fn from(h: V6) -> Self {
        Header::V6(h)
    }
}
