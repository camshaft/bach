use crate::{
    environment::net::{
        ip,
        monitor::List as Monitors,
        queue,
        socket::{self, udp::Socket as UdpSocket},
    },
    group::Group,
    net::{monitor::Monitor, IpAddr},
    scope::define,
};
use core::{future::Future, pin::pin, task::Context};
use std::{collections::HashMap, io};

use super::ip::{header, transport, Packet, Transport};
use bytes::Bytes;
use turmoil_net::shim::tokio::net::UdpSocket as TurmoilUdpSocket;
use turmoil_net::{EnterGuard, HostId, Net};

define!(scope, Box<Registry>);

pub(crate) fn with_registry<F: FnOnce(&mut Registry) -> io::Result<R>, R>(f: F) -> io::Result<R> {
    scope::try_borrow_mut_with(|registry| {
        if let Some(registry) = registry {
            f(registry)
        } else {
            Err(io::Error::other("No net registry in scope"))
        }
    })
}

pub struct Registry {
    hostnames: HashMap<String, (Group, IpAddr)>,
    group_ids: HashMap<Group, HostId>,
    ips: ip::Allocator,
    monitors: Monitors,
    #[allow(dead_code)]
    queue_alloc: Box<dyn queue::Allocator>,
    guard: Option<EnterGuard>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new(Box::<queue::Fixed>::default())
    }
}

impl Registry {
    pub fn new(queue: Box<dyn queue::Allocator>) -> Self {
        let monitors = Monitors::default();
        Self {
            hostnames: HashMap::new(),
            group_ids: HashMap::new(),
            ips: ip::Allocator::default(),
            queue_alloc: queue,
            monitors,
            guard: None,
        }
    }

    pub fn set_queue(&mut self, queue: Box<dyn queue::Allocator>) {
        self.queue_alloc = queue;
    }

    pub fn set_pcap_dir<P: Into<std::path::PathBuf>>(&mut self, pcap: P) -> io::Result<()> {
        let _ = pcap.into();
        Ok(())
    }

    pub fn set_subnet(&mut self, subnet: IpAddr) {
        self.ips = ip::Allocator::new(subnet);
    }

    pub fn set_monitor(&mut self, enabled: bool) {
        self.monitors.configure(enabled);
    }

    pub fn resolve_host(&mut self, group: &Group, name: &str) -> std::io::Result<IpAddr> {
        if name == "localhost" {
            let name = group.name();
            if name != "localhost" {
                return self.resolve_host(group, &name);
            }
        }

        if let Some((owner, ip)) = self.hostnames.get(name).copied() {
            if owner == *group || self.guard.is_none() {
                return Ok(ip);
            }
            return Ok(ip);
        }

        let group_name = group.name();

        // if the group name doesn't match the query, then it means it hasn't been allocated yet so return
        // an error.
        if group_name != name {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "hostname not found",
            ));
        }

        self.ensure_group_host(group)
    }

    pub fn register_monitor<M: Monitor>(&mut self, monitor: M) {
        self.monitors.push(monitor);
    }

    pub fn register_udp_socket(
        &mut self,
        group: &Group,
        options: &socket::Options,
    ) -> std::io::Result<Box<dyn socket::Socket>> {
        let group_ip = self.ensure_group_host(group)?;
        self.prepare()?;
        self.set_current_group(group)?;

        let mut local_addr = options.local_addr;

        if local_addr.ip().is_unspecified() || local_addr.ip().is_loopback() {
            local_addr.set_ip(group_ip);
        } else if local_addr.ip() != group_ip {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AddrNotAvailable,
                "invalid ip address",
            ));
        }

        self.monitors
            .on_socket_opened(&local_addr, transport::Kind::Udp)?;

        let socket = bind_udp_socket(local_addr)?;
        let socket = UdpSocket::new(socket, self.monitors.clone())?;
        Ok(Box::new(socket))
    }

    pub fn set_current_group(&mut self, group: &Group) -> io::Result<()> {
        self.prepare()?;
        let Some(host_id) = self.group_ids.get(group).copied() else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("group `{group}` is not registered in turmoil-net"),
            ));
        };
        turmoil_net::set_current(host_id);
        Ok(())
    }

    pub fn drain_packets(&self) -> Vec<turmoil_net::Packet> {
        let Some(guard) = self.guard.as_ref() else {
            return Vec::new();
        };

        let mut packets = Vec::new();
        guard.egress_all(&mut packets);
        packets
    }

    pub fn deliver(&self, packet: turmoil_net::Packet) {
        if let Some(guard) = self.guard.as_ref() {
            guard.deliver(packet);
        }
    }

    pub fn monitors(&self) -> Monitors {
        self.monitors.clone()
    }

    fn prepare(&mut self) -> io::Result<()> {
        if self.guard.is_some() {
            return Ok(());
        }

        let mut groups = crate::group::list();
        groups.sort_by_key(Group::id);

        for group in groups {
            let _ = self.ensure_group_host(&group)?;
        }

        let mut hosts = self
            .hostnames
            .values()
            .copied()
            .collect::<Vec<(Group, IpAddr)>>();
        hosts.sort_by_key(|(group, _)| group.id());

        let mut net = Net::new();
        let mut group_ids = HashMap::with_capacity(hosts.len());

        for (group, ip) in hosts {
            let host_id = net.add_host(ip);
            group_ids.insert(group, host_id);
        }

        self.group_ids = group_ids;
        self.guard = Some(net.enter());
        Ok(())
    }

    fn ensure_group_host(&mut self, group: &Group) -> io::Result<IpAddr> {
        let name = group.name();
        if let Some((_, ip)) = self.hostnames.get(&name).copied() {
            return Ok(ip);
        }

        if self.guard.is_some() {
            return Err(io::Error::other(
                "adding new groups after turmoil-net initialization is not yet supported",
            ));
        }

        let ip = self.ips.allocate();
        self.hostnames.insert(name, (*group, ip));
        Ok(ip)
    }
}

pub(crate) fn monitor_packet(packet: &turmoil_net::Packet) -> Option<Packet> {
    let transport = match &packet.payload {
        turmoil_net::Transport::Udp(udp) => Transport::Udp(transport::Udp {
            source: udp.src_port,
            destination: udp.dst_port,
            payload: Bytes::clone(&udp.payload),
            checksum: 0,
        }),
        turmoil_net::Transport::Tcp(_) => return None,
    };

    let header = match (packet.src, packet.dst) {
        (IpAddr::V4(source), IpAddr::V4(destination)) => header::V4 {
            source,
            destination,
            dscp: 0,
            ecn: 0,
            df: true,
            id: 0,
            ttl: packet.ttl,
        }
        .into(),
        (IpAddr::V6(source), IpAddr::V6(destination)) => header::V6 {
            source,
            destination,
            dscp: 0,
            ecn: 0,
            flow_label: 0,
            hop_limit: packet.ttl,
        }
        .into(),
        (IpAddr::V4(source), IpAddr::V6(destination)) => header::V6 {
            source: source.to_ipv6_mapped(),
            destination,
            dscp: 0,
            ecn: 0,
            flow_label: 0,
            hop_limit: packet.ttl,
        }
        .into(),
        (IpAddr::V6(source), IpAddr::V4(destination)) => header::V6 {
            source,
            destination: destination.to_ipv6_mapped(),
            dscp: 0,
            ecn: 0,
            flow_label: 0,
            hop_limit: packet.ttl,
        }
        .into(),
    };

    let mut packet = Packet { header, transport };
    packet.update_checksum();
    Some(packet)
}

fn bind_udp_socket(local_addr: std::net::SocketAddr) -> io::Result<TurmoilUdpSocket> {
    let mut future = pin!(TurmoilUdpSocket::bind(local_addr));
    let waker = crate::task::waker::noop();
    let mut cx = Context::from_waker(&waker);

    match Future::poll(future.as_mut(), &mut cx) {
        core::task::Poll::Ready(result) => result,
        core::task::Poll::Pending => Err(io::Error::other(
            "turmoil-net UDP bind unexpectedly returned pending",
        )),
    }
}
