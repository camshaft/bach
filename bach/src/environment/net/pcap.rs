use crate::time::Instant;
use std::io;

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

    fn as_pcap_record(&self) -> Record<&Self> {
        Record::new(Instant::now(), self)
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

pub struct Record<T> {
    timestamp: Instant,
    inner: T,
}

impl<T> Record<T> {
    pub fn new(timestamp: Instant, inner: T) -> Self {
        Self { timestamp, inner }
    }
}

impl<T: AsPcap> AsPcap for Record<T> {
    fn as_pcap<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        //= https://www.ietf.org/archive/id/draft-gharris-opsawg-pcap-01.html#section-5
        //#     1                   2                   3
        //#     0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
        //#     +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#   0 |                      Timestamp (Seconds)                      |
        //#     +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#   4 |            Timestamp (Microseconds or nanoseconds)            |
        //#     +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#   8 |                    Captured Packet Length                     |
        //#     +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#  12 |                    Original Packet Length                     |
        //#     +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        //#  16 /                                                               /
        //#     /                          Packet Data                          /
        //#     /                        variable length                        /
        //#     /                                                               /
        //#     +---------------------------------------------------------------+
        let timestamp = self.timestamp.elapsed_since_start();
        let secs: u32 = timestamp
            .as_secs()
            .try_into()
            .map_err(|_err| io::Error::new(io::ErrorKind::InvalidData, "max timestamp exceeded"))?;
        let nanos: u32 = timestamp.subsec_nanos();

        out.write_all(&secs.to_ne_bytes())?;
        out.write_all(&nanos.to_ne_bytes())?;

        let packet_len: u32 =
            self.inner.pcap_len()?.try_into().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "max payload size exceeded")
            })?;

        out.write_all(&packet_len.to_ne_bytes())?;
        out.write_all(&packet_len.to_ne_bytes())?;

        self.inner.as_pcap(out)?;

        Ok(())
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
