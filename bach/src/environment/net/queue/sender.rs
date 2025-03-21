use crate::net::SocketAddr;
use siphasher::sip::SipHasher13;
use slab::Slab;
use std::collections::VecDeque;

pub struct Map<V> {
    inner: Slab<V>,
    active: VecDeque<usize>,
    hasher: Hasher,
}

impl<V> Map<V>
where
    V: Clone,
{
    pub(super) fn new(local_addr: SocketAddr) -> Self {
        let hasher = Hasher::new(local_addr);
        Self {
            inner: Default::default(),
            active: Default::default(),
            hasher,
        }
    }

    pub(super) fn lookup(&self, remote_addr: SocketAddr) -> V {
        if self.active.len() <= 1 {
            let id = self.active[0];
            return self.inner[id].clone();
        }

        // we only need to hash the remote (source) since the local addr was included in
        // the initial state
        let hash = self.hasher.hash_remote_addr(remote_addr);
        let idx = hash as usize % self.active.len();
        let id = self.active[idx];
        self.inner[id].clone()
    }

    pub(super) fn reserve(&mut self, sender: V) -> usize {
        let id = self.inner.insert(sender);
        self.active.push_back(id);
        id
    }

    pub(super) fn remove(&mut self, id: usize) -> bool {
        self.inner.remove(id);
        self.active.retain(|v| *v != id);
        self.active.is_empty()
    }
}

struct Hasher(SipHasher13);

impl Hasher {
    fn new(local_addr: SocketAddr) -> Self {
        let hasher = match local_addr {
            SocketAddr::V4(addr) => {
                let ip = u32::from_le_bytes(addr.ip().octets());
                let port = addr.port().into();
                SipHasher13::new_with_keys(ip as _, port)
            }
            SocketAddr::V6(addr) => {
                let ip = u128::from_le_bytes(addr.ip().octets());
                let port = addr.port().into();
                SipHasher13::new_with_keys(ip as _, port)
            }
        };
        Self(hasher)
    }

    fn hash_remote_addr(&self, addr: SocketAddr) -> u64 {
        const ADDR_SPACE: usize = 16 + 2;
        let mut bytes = [0; ADDR_SPACE];
        let mut offset = 0;

        match addr {
            SocketAddr::V4(addr) => {
                let octets = addr.ip().octets();
                bytes[offset..offset + octets.len()].copy_from_slice(&octets);
                offset += octets.len();
            }
            SocketAddr::V6(addr) => {
                let octets = addr.ip().octets();
                bytes[offset..offset + octets.len()].copy_from_slice(&octets);
                offset += octets.len();
            }
        }

        bytes[offset..offset + 2].copy_from_slice(&addr.port().to_le_bytes());
        offset += 2;

        self.0.hash(&bytes[..offset])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bolero::check;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn hash_remote_addr() {
        check!()
            .with_type::<(SocketAddr, SocketAddr)>()
            .cloned()
            .for_each(|(local, remote)| {
                let hasher = Hasher::new(local);
                let _ = hasher.hash_remote_addr(remote);
            });
    }

    #[test]
    fn distribution_ipv4_test() {
        let mut map = Map::<u8>::new((Ipv4Addr::LOCALHOST, 8080).into());

        map.reserve(0);
        map.reserve(1);

        let mut counts = [0; 2];
        for port in 0..1000 {
            let v = map.lookup((Ipv4Addr::LOCALHOST, port).into());
            counts[v as usize] += 1;
        }

        assert_eq!(counts, [493, 507]);
    }

    #[test]
    fn distribution_ipv6_test() {
        let mut map = Map::<u8>::new((Ipv6Addr::LOCALHOST, 8080).into());

        map.reserve(0);
        map.reserve(1);

        let mut counts = [0; 2];
        for port in 0..1000 {
            let v = map.lookup((Ipv6Addr::LOCALHOST, port).into());
            counts[v as usize] += 1;
        }

        assert_eq!(counts, [524, 476]);
    }
}
