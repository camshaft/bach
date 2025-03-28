use super::ip::{transport, Packet};
use crate::net::monitor::{Command, DropReason, Monitor, Operation, SocketWrite};
use std::{
    io,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
pub struct List {
    monitors: Arc<Mutex<Vec<Box<dyn Monitor>>>>,
}

impl List {
    pub fn push<M: Monitor>(&self, monitor: M) {
        if let Ok(mut list) = self.monitors.lock() {
            list.push(Box::new(monitor));
        }
    }

    pub fn on_socket_opened(
        &self,
        local_addr: &SocketAddr,
        transport: transport::Kind,
    ) -> io::Result<()> {
        self.for_each_err(|monitor| monitor.on_socket_opened(local_addr, transport))
    }

    pub fn on_socket_closed(&self, local_addr: &SocketAddr, transport: transport::Kind) {
        self.for_each(|monitor| {
            monitor.on_socket_closed(local_addr, transport);
        });
    }

    pub fn on_socket_write(&self, socket_write: &SocketWrite) -> io::Result<()> {
        self.for_each_err(|monitor| monitor.on_socket_write(socket_write))
    }

    pub fn on_packet_sent(&self, packet: &Packet) -> Command {
        self.on_packet(packet, Operation::Send)
    }

    pub fn on_packet_received(&self, packet: &Packet) -> Command {
        self.on_packet(packet, Operation::Receive)
    }

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

    pub fn on_packet_dropped(&self, packet: &Packet, drop_reason: DropReason) {
        self.for_each(|m| {
            m.on_packet_dropped(packet, drop_reason);
        });
    }

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

    fn for_each<F: FnMut(&mut dyn Monitor)>(&self, mut f: F) {
        if let Ok(mut list) = self.monitors.lock() {
            for monitor in list.iter_mut() {
                f(&mut **monitor)
            }
        }
    }
}
