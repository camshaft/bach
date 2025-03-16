use crate::environment::net::{ip::Packet, pcap::AsPcap};
use std::io;

#[derive(Clone)]
pub struct Writer<O> {
    out: O,
}

impl<O> Writer<O>
where
    O: io::Write,
{
    pub fn new(mut out: O) -> io::Result<Self> {
        //= https://www.ietf.org/archive/id/draft-gharris-opsawg-pcap-01.html#section-4
        //#     1                   2                   3
        //#     0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#  0 |                          Magic Number                         |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#  4 |          Major Version        |         Minor Version         |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#  8 |                           Reserved1                           |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //# 12 |                           Reserved2                           |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //# 16 |                            SnapLen                            |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //# 20 | FCS |f|0 0 0 0 0 0 0 0 0 0 0 0|         LinkType              |
        //#    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

        //= https://www.ietf.org/archive/id/draft-gharris-opsawg-pcap-01.html#section-4
        //# Magic Number (32 bits):  an unsigned magic number, whose value is
        //#     either the hexadecimal number 0xA1B2C3D4 or the hexadecimal number
        //#     0xA1B23C4D.
        //#
        //#     If the value is 0xA1B2C3D4, time stamps in Packet Records (see
        //#     Figure 2) are in seconds and microseconds; if it is 0xA1B23C4D,
        //#     time stamps in Packet Records are in seconds and nanoseconds.
        let magic_number = 0xA1B23C4Du32;
        out.write_all(&magic_number.to_ne_bytes())?;

        //= https://www.ietf.org/archive/id/draft-gharris-opsawg-pcap-01.html#section-4
        //# Major Version (16 bits):  an unsigned value, giving the number of the
        //#     current major version of the format.  The value for the current
        //#     version of the format is 2.  This value should change if the
        //#     format changes in such a way that code that reads the new format
        //#     could not read the old format (i.e., code to read both formats
        //#     would have to check the version number and use different code
        //#     paths for the two formats) and code that reads the old format
        //#     could not read the new format.
        let major_version = 2u16;
        out.write_all(&major_version.to_ne_bytes())?;

        //= https://www.ietf.org/archive/id/draft-gharris-opsawg-pcap-01.html#section-4
        //#  Minor Version (16 bits):  an unsigned value, giving the number of the
        //#     current minor version of the format.  The value is for the current
        //#     version of the format is 4.  This value should change if the
        //#     format changes in such a way that code that reads the new format
        //#     could read the old format without checking the version number but
        //#     code that reads the old format could not read all files in the new
        //#     format.
        let minor_version = 4u16;
        out.write_all(&minor_version.to_ne_bytes())?;

        out.write_all(&0u32.to_ne_bytes())?;
        out.write_all(&0u32.to_ne_bytes())?;

        //= https://www.ietf.org/archive/id/draft-gharris-opsawg-pcap-01.html#section-4
        //# SnapLen (32 bits):  an unsigned value indicating the maximum number
        //#     of octets captured from each packet.  The portion of each packet
        //#     that exceeds this value will not be stored in the file.  This
        //#     value MUST NOT be zero; if no limit was specified, the value
        //#     should be a number greater than or equal to the largest packet
        //#     length in the file.
        let snaplen = u16::MAX as u32;
        out.write_all(&snaplen.to_ne_bytes())?;

        // ethernet
        out.write_all(&1u32.to_ne_bytes())?;

        Ok(Writer { out })
    }

    pub fn write_packet(&mut self, packet: &mut Packet) -> io::Result<()> {
        // make sure checksums are updated before emitting
        packet.update_checksum();

        packet.as_pcap_record().as_pcap(self)?;

        Ok(())
    }
}

impl<O> io::Write for Writer<O>
where
    O: io::Write,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.out.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}
