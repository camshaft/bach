use crate::time::Instant;
use std::io;

pub mod block;
mod dns;
mod queue;
mod registry;
mod writer;

pub use queue::{PcapQueue as Queue, QueueExt};
pub use registry::Registry;
pub use writer::Writer;

pub trait AsPcap {
    fn as_pcap<W: io::Write>(&self, out: &mut W) -> io::Result<()>;

    fn pcap_len(&self) -> io::Result<usize> {
        let mut len = LenEstimator::default();
        self.as_pcap(&mut len)?;
        Ok(len.finish())
    }

    fn as_pcap_record(&self) -> block::EnhancedPacket<&Self> {
        block::EnhancedPacket::new(Instant::now(), self)
    }

    fn as_pcap_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.as_pcap(&mut buf).unwrap();
        buf
    }
}

impl<T: AsPcap> AsPcap for &T {
    fn as_pcap<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        (*self).as_pcap(out)
    }
}

impl AsPcap for () {
    fn as_pcap<W: io::Write>(&self, _out: &mut W) -> io::Result<()> {
        Ok(())
    }
}

impl AsPcap for u8 {
    fn as_pcap<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        out.write_all(&[*self])
    }
}

impl AsPcap for u16 {
    fn as_pcap<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        out.write_all(&self.to_le_bytes())
    }
}

impl AsPcap for u32 {
    fn as_pcap<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        out.write_all(&self.to_le_bytes())
    }
}

impl AsPcap for [u8] {
    fn as_pcap<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        out.write_all(self)
    }
}

#[derive(Debug, Default)]
struct LenEstimator {
    len: usize,
}

impl LenEstimator {
    fn finish(self) -> usize {
        self.len
    }
}

impl io::Write for LenEstimator {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.len += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
