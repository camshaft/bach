use super::ip::{transport, Packet};
use crate::net::monitor::{
    Command, DropReason, Monitor, Offset, Operation, SocketRead, SocketWrite,
};
use std::{io, net::SocketAddr, sync, time::Duration};

type Inner = Vec<Box<dyn Monitor>>;

pub const DEFAULT_MAX_DUPLICATES: usize = 16;

#[derive(Clone)]
pub struct List {
    #[cfg(feature = "net-monitor")]
    monitors: Option<sync::Arc<sync::Mutex<Inner>>>,
    max_duplicates: usize,
}

impl Default for List {
    #[cfg(feature = "net-monitor")]
    fn default() -> Self {
        Self {
            monitors: Some(Default::default()),
            max_duplicates: DEFAULT_MAX_DUPLICATES,
        }
    }

    #[cfg(not(feature = "net-monitor"))]
    fn default() -> Self {
        Self {
            max_duplicates: DEFAULT_MAX_DUPLICATES,
        }
    }
}

impl List {
    #[inline]
    pub fn configure(&mut self, enabled: bool) {
        #[cfg(feature = "net-monitor")]
        {
            if enabled {
                self.monitors = Some(Default::default());
            } else {
                self.monitors = None;
            }
        }

        let _ = enabled;
    }

    #[inline]
    pub fn set_max_duplicates(&mut self, max: usize) {
        self.max_duplicates = max;
    }

    #[inline]
    pub fn push<M: Monitor>(&self, monitor: M) {
        if let Some(mut list) = self.lock() {
            list.push(Box::new(monitor));
        }
    }

    #[inline]
    pub fn on_socket_opened(
        &self,
        local_addr: &SocketAddr,
        transport: transport::Kind,
    ) -> io::Result<()> {
        self.for_each_err(|monitor| monitor.on_socket_opened(local_addr, transport))
    }

    #[inline]
    pub fn on_socket_closed(&self, local_addr: &SocketAddr, transport: transport::Kind) {
        self.for_each(|monitor| {
            monitor.on_socket_closed(local_addr, transport);
        });
    }

    #[inline]
    pub fn on_socket_write(&self, socket_write: &mut SocketWrite) -> io::Result<()> {
        self.for_each_err(|monitor| monitor.on_socket_write(socket_write))
    }

    #[inline]
    pub fn on_socket_read(&self, socket_read: &mut SocketRead) -> io::Result<()> {
        self.for_each_err(|monitor| monitor.on_socket_read(socket_read))
    }

    #[inline]
    pub fn on_packet_sent(&self, packet: &Packet) -> Option<PacketOutcome> {
        self.on_packet(packet, Operation::Send)
    }

    #[inline]
    pub fn on_packet_received(&self, packet: &Packet) -> Option<PacketOutcome> {
        self.on_packet(packet, Operation::Receive)
    }

    #[inline]
    pub fn on_packet(&self, packet: &Packet, operation: Operation) -> Option<PacketOutcome> {
        self.on_packet_with_budget(packet, operation, self.max_duplicates)
    }

    fn on_packet_with_budget(
        &self,
        packet: &Packet,
        operation: Operation,
        mut remaining: usize,
    ) -> Option<PacketOutcome> {
        let mut plan = PacketPlan::default();
        self.for_each(|m| {
            let command = m.on_packet(packet, operation);
            Self::apply_command(&mut plan, command);
        });

        if plan.is_drop {
            self.for_each(|m| {
                m.on_packet_dropped(packet, DropReason::Monitor);
            });
            return None;
        }

        let mut duplicates = Vec::new();

        for duplicate in plan.duplicates {
            let base = match duplicate.offset {
                Offset::Absolute => Duration::ZERO,
                Offset::Relative => plan.delay,
            };

            for _ in 0..duplicate.count {
                if remaining == 0 {
                    break;
                }
                remaining -= 1;

                let mut dup_packet = packet.clone();
                dup_packet.is_duplicate = true;

                if let Some(outcome) = self.on_packet_with_budget(&dup_packet, operation, remaining)
                {
                    remaining = remaining.saturating_sub(outcome.duplicates.len());
                    duplicates.push(ScheduledPacket {
                        packet: dup_packet,
                        delay: base + outcome.delay,
                    });
                    for mut nested in outcome.duplicates {
                        nested.delay += base;
                        duplicates.push(nested);
                    }
                }
            }
        }

        Some(PacketOutcome {
            delay: plan.delay,
            delay_offset: plan.delay_offset,
            duplicates,
        })
    }

    #[inline]
    pub fn on_packet_dropped(&self, packet: &Packet, drop_reason: DropReason) {
        self.for_each(|m| {
            m.on_packet_dropped(packet, drop_reason);
        });
    }

    #[inline]
    fn for_each_err<F: FnMut(&mut dyn Monitor) -> io::Result<()>>(
        &self,
        mut f: F,
    ) -> io::Result<()> {
        let mut res = Ok(());
        self.for_each(|monitor| {
            let v = f(monitor);
            if v.is_err() && res.is_ok() {
                res = v;
            }
        });
        res
    }

    #[inline]
    fn for_each<F: FnMut(&mut dyn Monitor)>(&self, mut f: F) {
        if let Some(mut list) = self.lock() {
            for monitor in list.iter_mut() {
                f(&mut **monitor)
            }
        }
    }

    fn apply_command(plan: &mut PacketPlan, command: Command) {
        match command {
            Command::Pass => {}
            Command::Drop => {
                plan.is_drop = true;
            }
            Command::Delay(delay) => {
                plan.delay += delay.duration;
                // once any monitor requests absolute, it latches
                if delay.offset == Offset::Absolute {
                    plan.delay_offset = delay.offset;
                }
            }
            Command::Duplicate(dup) => {
                if dup.count > 0 {
                    plan.duplicates.push(PlanDuplicate {
                        count: dup.count,
                        offset: dup.offset,
                    });
                }
            }
        }
    }

    #[cfg(feature = "net-monitor")]
    #[inline(always)]
    fn lock(&self) -> Option<sync::MutexGuard<'_, Inner>> {
        let inner = self.monitors.as_ref()?;
        inner.lock().ok()
    }

    #[cfg(not(feature = "net-monitor"))]
    #[inline(always)]
    fn lock(&self) -> Option<sync::MutexGuard<'_, Inner>> {
        None
    }
}

pub struct PacketOutcome {
    pub delay: Duration,
    pub delay_offset: Offset,
    pub duplicates: Vec<ScheduledPacket>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledPacket {
    pub packet: Packet,
    pub delay: Duration,
}

#[derive(Default)]
struct PacketPlan {
    delay: Duration,
    delay_offset: Offset,
    duplicates: Vec<PlanDuplicate>,
    is_drop: bool,
}

#[derive(Clone, Copy)]
struct PlanDuplicate {
    count: usize,
    offset: Offset,
}
