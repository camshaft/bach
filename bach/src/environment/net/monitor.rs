use super::ip::{transport, Packet};
use crate::net::monitor::{Command, DropReason, Monitor, Operation, SocketRead, SocketWrite};
use std::{io, net::SocketAddr, sync};

type Inner = Vec<Box<dyn Monitor>>;

#[derive(Clone)]
pub struct List {
    #[cfg(feature = "net-monitor")]
    monitors: Option<sync::Arc<sync::Mutex<Inner>>>,
}

impl Default for List {
    #[cfg(feature = "net-monitor")]
    fn default() -> Self {
        // If the feature is enabled then set it up by default with the ability to opt-out
        Self {
            monitors: Some(Default::default()),
        }
    }

    #[cfg(not(feature = "net-monitor"))]
    fn default() -> Self {
        Self {}
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
    pub fn on_packet_sent(&self, packet: &Packet) -> Command {
        self.on_packet(packet, Operation::Send)
    }

    #[inline]
    pub fn on_packet_received(&self, packet: &Packet) -> Command {
        self.on_packet(packet, Operation::Receive)
    }

    #[inline]
    pub fn on_packet(&self, packet: &Packet, operation: Operation) -> Command {
        let mut command = Command::Pass;
        self.for_each(|m| {
            let c = m.on_packet(packet, operation);
            if matches!(c, Command::Drop) {
                command = c;
            }
        });

        if command.is_drop() {
            self.for_each(|m| {
                m.on_packet_dropped(packet, DropReason::Monitor);
            });
        }

        command
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

    #[cfg(feature = "net-monitor")]
    #[inline(always)]
    fn lock(&self) -> Option<sync::MutexGuard<Inner>> {
        let inner = self.monitors.as_ref()?;
        inner.lock().ok()
    }

    #[cfg(not(feature = "net-monitor"))]
    #[inline(always)]
    fn lock(&self) -> Option<sync::MutexGuard<Inner>> {
        None
    }
}
