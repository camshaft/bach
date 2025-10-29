use super::Packet;

#[derive(Clone, Debug)]
pub struct Segments {
    pub packet: Packet,
    pub segment_len: usize,
}

impl IntoIterator for Segments {
    type Item = Packet;
    type IntoIter = Iter;

    fn into_iter(self) -> Self::IntoIter {
        Iter {
            packet: Some(self.packet),
            segment_len: self.segment_len,
        }
    }
}

pub struct Iter {
    packet: Option<Packet>,
    segment_len: usize,
}

impl Iterator for Iter {
    type Item = Packet;

    fn next(&mut self) -> Option<Self::Item> {
        let packet = self.packet.as_mut()?;

        let split_len = packet.transport.payload().len().min(self.segment_len);

        let payload = packet.transport.payload_mut().split_to(split_len);

        let is_last = packet.transport.payload().is_empty();

        // Avoid cloning on the last segment by taking ownership
        let mut result_packet = if is_last {
            self.packet.take()?
        } else {
            packet.clone()
        };
        *result_packet.transport.payload_mut() = payload;

        Some(result_packet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::net::ip::{transport::Udp, *};
    use bytes::Bytes;

    #[test]
    fn segments_order() {
        let packet = Packet {
            header: header::V4 {
                source: Ipv4Addr::LOCALHOST,
                destination: Ipv4Addr::LOCALHOST,
                ..Default::default()
            }
            .into(),
            transport: Udp {
                source: 42,
                destination: 42,
                payload: Bytes::from_static(b"0123456789"),
                checksum: 0,
            }
            .into(),
        };

        let segments = Segments {
            packet,
            segment_len: 1,
        };

        for (idx, segment) in segments.into_iter().enumerate() {
            let payload = segment.transport.payload();
            assert_eq!(payload.len(), 1);
            assert_eq!(payload[0] as usize, '0' as usize + idx);
        }
    }
}
