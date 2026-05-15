use crate::{
    environment::net::{
        ip::{Packet, Segments},
        monitor::List as Monitors,
        pcap::{self, QueueExt as _},
    },
    ext::*,
    group::Group,
    net::{
        monitor::{DropReason, Offset},
        SocketAddr,
    },
    queue::{latent, vec_deque},
    sync::channel::{Receiver, Sender},
};
use alloc::sync::Arc;
use core::{fmt, time::Duration};
use sender::SenderId;
use std::{
    collections::{hash_map::Entry, HashMap},
    io,
    sync::Mutex,
};

mod sender;

type SenderMap = sender::Map<Sender<Packet>>;

#[derive(Clone)]
pub struct Dispatch {
    inner: Arc<Mutex<HashMap<SocketAddr, SenderMap>>>,
    monitors: Monitors,
}

impl Dispatch {
    pub(crate) fn new(monitors: Monitors) -> Self {
        Self {
            inner: Default::default(),
            monitors,
        }
    }

    pub async fn send(&self, packet: Packet) {
        let Some(outcome) = self.monitors.on_packet_received(&packet) else {
            return;
        };

        let sender = if let Ok(inner) = self.inner.lock() {
            if let Some(senders) = inner.get(&packet.destination()) {
                senders.lookup(packet.source())
            } else {
                count!("packet_dropped", 1);
                self.monitors
                    .on_packet_dropped(&packet, DropReason::UnknownDestination);
                return;
            }
        } else {
            count!("packet_dropped", 1);
            return;
        };

        Self::deliver(sender.clone(), packet, outcome.delay, &self.monitors);

        for dup in outcome.duplicates {
            Self::deliver(sender.clone(), dup.packet, dup.delay, &self.monitors);
        }
    }

    fn deliver(mut sender: Sender<Packet>, packet: Packet, delay: Duration, monitors: &Monitors) {
        if delay.is_zero() {
            let monitors = monitors.clone();
            async move {
                if let Ok(Some(prev)) = sender.push_nowait(packet).await {
                    monitors.on_packet_dropped(&prev, DropReason::ReceiveBufferFull);
                    count!("packet_dropped", 1);
                }
            }
            .spawn();
        } else {
            let monitors = monitors.clone();
            async move {
                crate::time::sleep(delay).await;
                if let Ok(Some(prev)) = sender.push_nowait(packet).await {
                    monitors.on_packet_dropped(&prev, DropReason::ReceiveBufferFull);
                    count!("packet_dropped", 1);
                }
            }
            .spawn();
        }
    }

    pub fn reserve(&self, addr: SocketAddr, sender: Sender<Packet>) -> Reservation {
        let id = self
            .inner
            .lock()
            .unwrap()
            .entry(addr)
            .or_insert_with(|| SenderMap::new(addr))
            .reserve(sender);
        Reservation {
            addr,
            id,
            dispatch: self.clone(),
        }
    }
}

pub struct Reservation {
    addr: SocketAddr,
    id: SenderId,
    dispatch: Dispatch,
}

impl fmt::Debug for Reservation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Reservation").field(&self.addr).finish()
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let Ok(mut inner) = self.dispatch.inner.lock() else {
            return;
        };

        let Entry::Occupied(mut senders) = inner.entry(self.addr) else {
            return;
        };

        let is_empty = senders.get_mut().remove(self.id);

        if is_empty {
            senders.remove();
        }
    }
}

pub trait Allocator {
    fn for_udp(
        &mut self,
        group: &Group,
        addr: SocketAddr,
        dispatch: &Dispatch,
        monitors: &Monitors,
        pcaps: &mut pcap::Registry,
    ) -> PacketQueue;
}

pub struct PacketQueue {
    pub local_sender: Sender<Segments>,
    pub local_receiver: Receiver<Packet>,
    pub remote_sender: Sender<Packet>,
}

// TODO port the `Model` from s2n-quic

pub struct Fixed {
    tx_packet_limit: Option<usize>,
    rx_packet_limit: Option<usize>,
    inflight_limit: Option<usize>,
    net_latency: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InflightPacket {
    packet: Packet,
    delay: Duration,
    delay_offset: Offset,
}

impl pcap::Record for InflightPacket {
    fn write_pcap_record<O: std::io::Write>(
        &mut self,
        writer: &mut pcap::Writer<O>,
    ) -> io::Result<()> {
        writer.write_packet(&mut self.packet)
    }
}

#[derive(Clone, Copy)]
struct NetLatency(Duration);

impl latent::Latency<InflightPacket> for NetLatency {
    fn for_value(&self, value: &InflightPacket) -> Duration {
        match value.delay_offset {
            Offset::Relative => self.0 + value.delay,
            Offset::Absolute => value.delay,
        }
    }
}

impl Default for Fixed {
    fn default() -> Self {
        Self {
            tx_packet_limit: Some(4096),
            rx_packet_limit: Some(4096),
            inflight_limit: Some(u16::MAX as _),
            net_latency: Duration::from_millis(50),
        }
    }
}

impl Fixed {
    pub fn with_tx_packet_limit(mut self, limit: Option<usize>) -> Self {
        self.tx_packet_limit = limit;
        self
    }

    pub fn with_rx_packet_limit(mut self, limit: Option<usize>) -> Self {
        self.rx_packet_limit = limit;
        self
    }

    pub fn with_inflight_limit(mut self, limit: Option<usize>) -> Self {
        self.inflight_limit = limit;
        self
    }

    pub fn with_net_latency(mut self, latency: Duration) -> Self {
        self.net_latency = latency;
        self
    }
}

impl Allocator for Fixed {
    fn for_udp(
        &mut self,
        group: &Group,
        addr: SocketAddr,
        dispatch: &Dispatch,
        monitors: &Monitors,
        pcaps: &mut pcap::Registry,
    ) -> PacketQueue {
        let (tx_sender, mut tx_receiver) = vec_deque::Queue::builder()
            .with_capacity(self.tx_packet_limit)
            .with_overflow(vec_deque::Overflow::PreferOldest)
            .build()
            .sojourn()
            .span(format!("udp://{addr}/tx"))
            .mutex()
            .channel();

        let _: &Sender<Segments> = &tx_sender;

        let pcap = pcaps.open(group);

        let rx = vec_deque::Queue::builder()
            .with_capacity(self.rx_packet_limit)
            .with_overflow(vec_deque::Overflow::PreferOldest)
            .build()
            .sojourn()
            .span(format!("udp://{addr}/rx"));

        let (rx_sender, rx_receiver) = if let Some(pcap) = &pcap {
            rx.pcap_push(pcap.clone()).mutex().channel()
        } else {
            rx.mutex().channel()
        };

        let net = vec_deque::Queue::builder()
            .with_capacity(self.inflight_limit)
            .with_overflow(vec_deque::Overflow::PreferOldest)
            .build()
            .latent(NetLatency(self.net_latency))
            .span(format!("udp://{addr}/net"));

        let (mut net_send, mut net_recv) = if let Some(pcap) = pcap {
            net.pcap_push(pcap).mutex().channel()
        } else {
            net.mutex().channel()
        };

        {
            let monitors = monitors.clone();
            async move {
                'packets: while let Ok(segments) = tx_receiver.recv().await {
                    for packet in segments {
                        let Some(outcome) = monitors.on_packet_sent(&packet) else {
                            continue;
                        };

                        let inflight = InflightPacket {
                            packet,
                            delay: outcome.delay,
                            delay_offset: outcome.delay_offset,
                        };
                        if net_send.push_nowait(inflight).await.is_err() {
                            break 'packets;
                        }

                        for dup in outcome.duplicates {
                            let inflight = InflightPacket {
                                packet: dup.packet,
                                delay: dup.delay,
                                delay_offset: outcome.delay_offset,
                            };
                            if net_send.push_nowait(inflight).await.is_err() {
                                break 'packets;
                            }
                        }
                    }
                }
                let _ = tx_receiver.close();
            }
            .spawn_named(format_args!("udp://{addr}/net/local"));
        }

        let senders = dispatch.clone();
        async move {
            while let Ok(inflight) = net_recv.recv().await {
                senders.send(inflight.packet).await;
            }
            let _ = net_recv.close();
        }
        .spawn_named(format_args!("udp://{addr}/net/remote"));

        PacketQueue {
            local_sender: tx_sender,
            local_receiver: rx_receiver,
            remote_sender: rx_sender,
        }
    }
}
