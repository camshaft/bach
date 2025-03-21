use crate::environment::net::{
    ip::Packet,
    pcap::{block, AsPcap},
};
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
        block::SectionHeader::default().as_pcap(&mut out)?;

        block::InterfaceDescription::default().as_pcap(&mut out)?;

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
