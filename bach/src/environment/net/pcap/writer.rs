use crate::environment::net::{
    ip::Packet,
    pcap::{block, AsPcap, Record},
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

        self.write_record(packet)?;

        Ok(())
    }

    pub fn write_record<T: AsPcap>(&mut self, packet: &T) -> io::Result<()> {
        packet.as_pcap_record().as_pcap(self)
    }
}

impl Record for Packet {
    fn write_pcap_record<W: io::Write>(&mut self, writer: &mut Writer<W>) -> io::Result<()> {
        writer.write_packet(self)
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
