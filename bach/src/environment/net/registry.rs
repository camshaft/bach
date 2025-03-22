use crate::{
    environment::net::{
        ip, port,
        queue::{self, Dispatch},
        socket::{self, reservation},
    },
    group::Group,
    net::IpAddr,
    scope::define,
};
use std::{collections::HashMap, io};

use super::pcap;

define!(scope, Box<Registry>);

pub(crate) fn with_registry<F: FnOnce(&mut Registry) -> io::Result<R>, R>(f: F) -> io::Result<R> {
    scope::try_borrow_mut_with(|registry| {
        if let Some(registry) = registry {
            f(registry)
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "No net registry in scope",
            ))
        }
    })
}

pub struct Registry {
    hostnames: HashMap<String, (Group, IpAddr)>,
    senders: Dispatch,
    groups: HashMap<Group, GroupState>,
    ips: ip::Allocator,
    pcaps: pcap::Registry,
    queue_alloc: Box<dyn queue::Allocator>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new(Box::<queue::Fixed>::default())
    }
}

impl Registry {
    pub fn new(queue: Box<dyn queue::Allocator>) -> Self {
        Self {
            hostnames: HashMap::new(),
            senders: Default::default(),
            groups: HashMap::new(),
            ips: ip::Allocator::default(),
            pcaps: Default::default(),
            queue_alloc: queue,
        }
    }

    pub fn set_queue(&mut self, queue: Box<dyn queue::Allocator>) {
        self.queue_alloc = queue;
    }

    pub fn set_pcap_dir<P: Into<std::path::PathBuf>>(&mut self, pcap: P) -> io::Result<()> {
        self.pcaps.set_dir(pcap)
    }

    pub fn set_subnet(&mut self, subnet: IpAddr) {
        self.ips = ip::Allocator::new(subnet);
    }

    pub fn resolve_host(&mut self, group: &Group, name: &str) -> std::io::Result<IpAddr> {
        if name == "localhost" {
            let name = group.name();
            if name != "localhost" {
                return self.resolve_host(group, &name);
            }
        }

        if let Some((owner, ip)) = self.hostnames.get(name).cloned() {
            // the owner would have already resolved itself in the pcap
            if owner == *group {
                return Ok(ip);
            }

            let group_name = group.name();

            // inject a DNS packet in the pcap
            let first_time = self.pcaps.dns(group, name, &ip);

            // if this is the first time `group` has queried `name`, then do a reverse query
            // on the owner so the pcaps come through correctly on the other side
            if first_time {
                let _ = self.resolve_host(&owner, &group_name);
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

        let ip = self.ips.allocate();
        self.hostnames.insert(group_name, (*group, ip));
        self.groups.insert(*group, GroupState::default());

        self.pcaps.dns(group, name, &ip);

        Ok(ip)
    }

    pub fn register_udp_socket(
        &mut self,
        group: &Group,
        options: &socket::Options,
    ) -> std::io::Result<Box<dyn socket::Socket>> {
        let group_ip = self.resolve_host(group, &group.name())?;

        let mut local_addr = options.local_addr;

        if local_addr.ip().is_unspecified() || local_addr.ip().is_loopback() {
            local_addr.set_ip(group_ip);
        } else if local_addr.ip() != group_ip {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AddrNotAvailable,
                "invalid ip address",
            ));
        }

        let state = self.groups.get_mut(group).unwrap();

        let reservation = if local_addr.port() == 0 {
            let res = state.udp.ephemeral()?;
            local_addr.set_port(res.port());
            res
        } else {
            state.udp.reserve(local_addr.port(), options.reuse_port)?
        };

        let queue::PacketQueue {
            local_sender: sender,
            local_receiver: receiver,
            remote_sender,
        } = self
            .queue_alloc
            .for_udp(group, local_addr, &self.senders, &mut self.pcaps);

        let reservation = (reservation, self.senders.reserve(local_addr, remote_sender));
        let socket = socket::udp::Socket::new(sender, receiver, local_addr);
        let socket = reservation::Socket::new(socket, reservation);

        Ok(Box::new(socket))
    }
}

#[derive(Default)]
struct GroupState {
    udp: port::Allocator,
}
