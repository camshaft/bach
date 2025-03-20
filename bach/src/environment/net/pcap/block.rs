use crate::{environment::net::pcap::AsPcap, time::Instant};
use std::{io, net::IpAddr};

#[derive(Default)]
pub struct SectionHeader(());

impl AsPcap for SectionHeader {
    fn as_pcap<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        //= https://www.ietf.org/archive/id/draft-ietf-opsawg-pcapng-03.html#section-4.1
        //#     1                   2                   3
        //#     0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#  0 |                   Block Type = 0x0A0D0D0A                     |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#  4 |                      Block Total Length                       |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#  8 |                      Byte-Order Magic                         |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //# 12 |          Major Version        |         Minor Version         |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //# 16 |                                                               |
        //#    |                          Section Length                       |
        //#    |                                                               |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //# 24 /                                                               /
        //#    /                      Options (variable)                       /
        //#    /                                                               /
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#    |                      Block Total Length                       |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

        let magic_number = 0x0A0D0D0Au32;
        magic_number.as_pcap(out)?;

        let block_len = 4u32 * 8;
        block_len.as_pcap(out)?;

        //= https://www.ietf.org/archive/id/draft-ietf-opsawg-pcapng-03.html#section-4.1
        //# *  Byte-Order Magic (32 bits): an unsigned magic number, whose value
        //#     is the hexadecimal number 0x1A2B3C4D.  This number can be used to
        //#     distinguish sections that have been saved on little-endian
        //#     machines from the ones saved on big-endian machines, and to
        //#     heuristically identify pcapng files.
        let bom = 0x1A2B3C4Du32;
        bom.as_pcap(out)?;

        //= https://www.ietf.org/archive/id/draft-ietf-opsawg-pcapng-03.html#section-4.1
        //# *  Major Version (16 bits): an unsigned integer, giving the number of
        //#     the current major version of the format.  The value for the
        //#     current version of the format is 1 (big-endian 0x00 0x01 or
        //#     little-endian 0x01 0x00).
        let major_version = 1u16;
        major_version.as_pcap(out)?;

        //= https://www.ietf.org/archive/id/draft-ietf-opsawg-pcapng-03.html#section-4.1
        //# *  Minor Version (16 bits): an unsigned integer, giving the number of
        //#     the current minor version of the format.  The value for the
        //#     current version of the format is 0.
        let minor_version = 0u16;
        minor_version.as_pcap(out)?;

        //= https://www.ietf.org/archive/id/draft-ietf-opsawg-pcapng-03.html#section-4.1
        //# *  Section Length (64 bits): a signed integer specifying the length
        //#     in octets of the following section, excluding the Section Header
        //#     Block itself.  This field can be used to skip the section, for
        //#     faster navigation inside large files.  If the Section Length is -1
        //#     (0xFFFFFFFFFFFFFFFF), this means that the size of the section is
        //#     not specified, and the only way to skip the section is to parse
        //#     the blocks that it contains.
        let section_len = -1i64;
        out.write_all(&section_len.to_le_bytes())?;

        Opt::end().as_pcap(out)?;

        block_len.as_pcap(out)?;

        Ok(())
    }
}

#[derive(Default)]
pub struct InterfaceDescription {}

impl AsPcap for InterfaceDescription {
    fn as_pcap<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        //= https://www.ietf.org/archive/id/draft-ietf-opsawg-pcapng-03.html#section-4.2
        //#     1                   2                   3
        //#     0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#  0 |                   Block Type = 0x00000001                    |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#  4 |                      Block Total Length                       |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#  8 |           LinkType            |           Reserved            |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //# 12 |                            SnapLen                            |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //# 16 /                                                               /
        //#    /                      Options (variable)                       /
        //#    /                                                               /
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#    |                      Block Total Length                       |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

        let block_type = 0x1u32;
        block_type.as_pcap(out)?;

        let block_len = 4u32 * 8;
        block_len.as_pcap(out)?;

        //= https://www.ietf.org/archive/id/draft-ietf-opsawg-pcaplinktype-08.html#section-3.2.1
        //# Name  LINKTYPE_ETHERNET
        //# Number  1
        //# Description  IEEE 802.3 Ethernet
        let link_type = 1u16;
        link_type.as_pcap(out)?;
        let reserved = 0u16;
        reserved.as_pcap(out)?;

        //= https://www.ietf.org/archive/id/draft-ietf-opsawg-pcapng-03.html#section-4.2
        //# *  SnapLen (32 bits): an unsigned integer indicating the maximum
        //#     number of octets captured from each packet.  The portion of each
        //#     packet that exceeds this value will not be stored in the file.  A
        //#     value of zero indicates no limit.
        let snap_len = 0u32;
        snap_len.as_pcap(out)?;

        // use nanos
        Opt::if_tsresol(9).as_pcap(out)?;

        Opt::end().as_pcap(out)?;

        block_len.as_pcap(out)?;

        Ok(())
    }
}

pub struct EnhancedPacket<T> {
    timestamp: Instant,
    inner: T,
}

impl<T> EnhancedPacket<T> {
    pub fn new(timestamp: Instant, inner: T) -> Self {
        Self { timestamp, inner }
    }
}

impl<T: AsPcap> AsPcap for EnhancedPacket<T> {
    fn as_pcap<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        //= https://www.ietf.org/archive/id/draft-ietf-opsawg-pcapng-03.html#section-4.3
        //#     1                   2                   3
        //#     0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#  0 |                    Block Type = 0x00000006                    |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#  4 |                      Block Total Length                       |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#  8 |                         Interface ID                          |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //# 12 |                       (Upper 32 bits)                         |
        //#    + - - - - - - - - - - - -  Timestamp  - - - - - - - - - - - - - +
        //# 16 |                       (Lower 32 bits)                         |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //# 20 |                    Captured Packet Length                     |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //# 24 |                    Original Packet Length                     |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //# 28 /                                                               /
        //#    /                          Packet Data                          /
        //#    /              variable length, padded to 32 bits               /
        //#    /                                                               /
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#    /                                                               /
        //#    /                      Options (variable)                       /
        //#    /                                                               /
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#    |                      Block Total Length                       |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

        let block_type = 0x6u32;
        block_type.as_pcap(out)?;

        let packet_len = self.inner.pcap_len()?;
        let padding = padding_len(packet_len);

        let total_len = 4 * 9 + packet_len + padding;

        let total_len: u32 = total_len
            .try_into()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "max payload size exceeded"))?;

        let packet_len = packet_len as u32;

        total_len.as_pcap(out)?;

        let interface_id = 0u32;
        interface_id.as_pcap(out)?;

        //= https://www.ietf.org/archive/id/draft-ietf-opsawg-pcapng-03.html#section-4.2
        //# if_tsresol:
        //#     The if_tsresol option identifies the resolution of
        //#     timestamps.  If the Most Significant Bit is equal to zero,
        //#     the remaining bits indicates the resolution of the timestamp
        //#     as a negative power of 10 (e.g. 6 means microsecond
        //#     resolution, timestamps are the number of microseconds since
        //#     1970-01-01 00:00:00 UTC).  If the Most Significant Bit is
        //#     equal to one, the remaining bits indicates the resolution as
        //#     negative power of 2 (e.g. 10 means 1/1024 of second).  If
        //#     this option is not present, a resolution of 10^-6 is assumed
        //#     (i.e. timestamps have the same resolution of the standard
        //#     'libpcap' timestamps).
        let timestamp = self.timestamp.elapsed_since_start();

        //= https://www.ietf.org/archive/id/draft-ietf-opsawg-pcapng-03.html#section-4.3
        //# *  Timestamp (64 bits): two 32-bit unsigned integers, representing a
        //#     single 64-bit unsigned integer, with the first value being the
        //#     upper 32 bits of that integer and the second value being the lower
        //#     32 bits of that integer.  The 64-bit unsigned integer is a count
        //#     of units of time.
        let nanos: u64 = timestamp
            .as_nanos()
            .try_into()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "max timestamp exceeded"))?;

        let upper = (nanos >> 32) as u32;
        let lower = nanos as u32;
        upper.as_pcap(out)?;
        lower.as_pcap(out)?;

        // captured len
        packet_len.as_pcap(out)?;
        // original len
        packet_len.as_pcap(out)?;

        self.inner.as_pcap(out)?;

        // make sure the packet is padded to the nearest 4 bytes
        if padding > 0 {
            out.write_all(&[0u8; 4][..padding])?;
        }

        // TODO use epb_packet_id

        //= https://www.ietf.org/archive/id/draft-ietf-opsawg-pcapng-03.html#section-4.3
        //# epb_packetid:
        //#    The epb_packetid option is a 64-bit unsigned integer that
        //#    uniquely identifies the packet.  If the same packet is seen
        //#    by multiple interfaces and there is a way for the capture
        //#    application to correlate them, the same epb_packetid value
        //#    must be used.  An example could be a router that captures
        //#    packets on all its interfaces in both directions.  When a
        //#    packet hits interface A on ingress, an EPB entry gets
        //#    created, TTL gets decremented, and right before it egresses
        //#    on interface B another EPB entry gets created in the trace
        //#    file.  In this case, two packets are in the capture file,
        //#    which are not identical but the epb_packetid can be used to
        //#    correlate them.

        Opt::end().as_pcap(out)?;

        total_len.as_pcap(out)?;

        Ok(())
    }
}

pub struct NameResolution<'a> {
    pub addr: IpAddr,
    pub name: &'a str,
}

impl AsPcap for NameResolution<'_> {
    fn as_pcap<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        //= https://www.ietf.org/archive/id/draft-ietf-opsawg-pcapng-03.html#section-4.5
        //#     1                   2                   3
        //#     0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#  0 |                    Block Type = 0x00000004                    |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#  4 |                      Block Total Length                       |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#  8 |      Record Type              |      Record Value Length      |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //# 12 /                       Record Value                            /
        //#    /              variable length, padded to 32 bits               /
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#    .                                                               .
        //#    .                  . . . other records . . .                    .
        //#    .                                                               .
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#    |  Record Type = nrb_record_end |   Record Value Length = 0     |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#    /                                                               /
        //#    /                      Options (variable)                       /
        //#    /                                                               /
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#    |                      Block Total Length                       |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        let block_type = 0x4u32;
        block_type.as_pcap(out)?;

        let name_len = self.name.len() + 1 /* null-terminated */;
        let padding = padding_len(name_len);

        let addr_len = match self.addr {
            IpAddr::V4(_) => 4,
            IpAddr::V6(_) => 16,
        };

        let block_len = 4 * 6 + addr_len + name_len + padding;

        let block_len: u32 = block_len
            .try_into()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "max payload size exceeded"))?;

        block_len.as_pcap(out)?;

        let name_len = name_len as u16;

        //= https://www.ietf.org/archive/id/draft-ietf-opsawg-pcapng-03.html#section-4.5
        //# +==================+========+==========+
        //# | Name             | Type   | Length   |
        //# +==================+========+==========+
        //# | nrb_record_end   | 0x0000 | 0        |
        //# +------------------+--------+----------+
        //# | nrb_record_ipv4  | 0x0001 | variable |
        //# +------------------+--------+----------+
        //# | nrb_record_ipv6  | 0x0002 | variable |
        //# +------------------+--------+----------+
        match self.addr {
            IpAddr::V4(addr) => {
                let record_type = 1u16;
                record_type.as_pcap(out)?;

                let value_len = 4u16 + name_len;
                value_len.as_pcap(out)?;

                addr.octets().as_pcap(out)?;
            }
            IpAddr::V6(addr) => {
                let record_type = 2u16;
                record_type.as_pcap(out)?;

                let value_len = 16u16 + name_len;
                value_len.as_pcap(out)?;

                addr.octets().as_pcap(out)?;
            }
        }

        self.name.as_bytes().as_pcap(out)?;
        0u8.as_pcap(out)?;

        // make sure the packet is padded to the nearest 4 bytes
        if padding > 0 {
            out.write_all(&[0u8; 4][..padding])?;
        }

        let nrb_record_end = 0u32;
        nrb_record_end.as_pcap(out)?;

        Opt::end().as_pcap(out)?;

        block_len.as_pcap(out)?;

        Ok(())
    }
}

pub struct Opt<V> {
    pub ty: u16,
    pub value: V,
}

impl Opt<()> {
    pub fn end() -> Self {
        Self { ty: 0, value: () }
    }
}

impl Opt<u8> {
    //= https://www.ietf.org/archive/id/draft-ietf-opsawg-pcapng-03.html#section-4.2
    //# | if_tsresol     | 9    | 1                   | no                |
    pub fn if_tsresol(value: u8) -> Self {
        Self { ty: 9, value }
    }
}

impl<V: AsPcap> AsPcap for Opt<V> {
    fn as_pcap<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        self.ty.as_pcap(out)?;

        let len = self.value.pcap_len()?;
        let len: u16 = len
            .try_into()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "option len too large"))?;

        len.as_pcap(out)?;

        self.value.as_pcap(out)?;

        let padding = padding_len(len as _);

        if padding > 0 {
            out.write_all(&[0u8; 4][..padding])?;
        }

        Ok(())
    }
}

fn padding_len(len: usize) -> usize {
    let padding = 4 - (len % 4);
    if padding < 4 {
        padding
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bolero::{check, TypeGenerator};
    use pcap_parser::pcapng;

    #[derive(Clone, Copy, Debug, TypeGenerator)]
    enum Inner {
        U8(u8),
        U16(u16),
        U32(u32),
    }

    impl AsPcap for Inner {
        fn as_pcap<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
            match self {
                Inner::U8(v) => v.as_pcap(out),
                Inner::U16(v) => v.as_pcap(out),
                Inner::U32(v) => v.as_pcap(out),
            }
        }
    }

    fn block_checks<T: AsPcap>(v: &T) -> Vec<u8> {
        let mut out = Vec::new();
        v.as_pcap(&mut out).unwrap();

        let len = v.pcap_len().unwrap();
        // make sure the estimated lengths match
        assert_eq!(out.len(), len);

        // blocks should be padded
        assert_eq!(out.len() % 4, 0);

        // parse the block length and make sure it matches
        let parsed_total_len = u32::from_le_bytes(out[4..8].try_into().unwrap());
        assert_eq!(parsed_total_len as usize, len);

        // the last 4 bytes should also contain the length
        assert_eq!(out[4..8], out[len - 4..]);

        out
    }

    #[test]
    fn section_header() {
        let header = SectionHeader::default();
        let out = block_checks(&header);
        let (_, packet) = pcapng::parse_sectionheaderblock(&out).unwrap();
        dbg!(packet);
    }

    #[test]
    fn interface_description() {
        let interface = InterfaceDescription::default();
        let out = block_checks(&interface);
        let (_, packet) = pcapng::parse_interfacedescriptionblock_le(&out).unwrap();
        dbg!(packet);
    }

    #[test]
    fn enhanced_packet() {
        check!().with_type::<Inner>().for_each(|inner| {
            let time = crate::time::Instant::zero();
            let packet = EnhancedPacket::new(time, inner);
            let out = block_checks(&packet);
            let (_, packet) = pcapng::parse_enhancedpacketblock_le(&out).unwrap();
            let _ = packet;
        })
    }

    #[test]
    fn name_resolution() {
        check!()
            .with_type::<(IpAddr, String)>()
            .for_each(|(addr, name)| {
                let name = name.trim_end_matches('\0');
                let name_resolution = NameResolution { addr: *addr, name };
                let out = block_checks(&name_resolution);
                let (_, packet) = pcapng::parse_nameresolutionblock_le(&out).unwrap();
                let _ = packet;
            })
    }

    #[test]
    fn opt_encoding() {
        check!()
            .with_type::<(u16, Inner)>()
            .cloned()
            .for_each(|(ty, value)| {
                let opt = Opt { ty, value };
                let mut out = vec![];
                opt.as_pcap(&mut out).unwrap();
                let (_, opt) = pcapng::parse_option_le::<()>(&out).unwrap();
                let _ = opt;
            })
    }
}
