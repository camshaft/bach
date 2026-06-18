use crate::net::SocketAddr;
use siphasher::sip::SipHasher13;
use slotmap::{new_key_type, SlotMap};
use std::collections::VecDeque;

new_key_type! {
   pub struct SenderId;
}

pub struct Map<V> {
    inner: SlotMap<SenderId, V>,
    active: VecDeque<SenderId>,
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

    pub(super) fn reserve(&mut self, sender: V) -> SenderId {
        let id = self.inner.insert(sender);
        self.active.push_back(id);
        id
    }

    pub(super) fn remove(&mut self, id: SenderId) -> bool {
        self.inner.remove(id);
        // Optimize: find and remove the specific element instead of using retain
        if let Some(pos) = self.active.iter().position(|v| *v == id) {
            self.active.remove(pos);
        }
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
        match addr {
            SocketAddr::V4(addr) => {
                // 4 bytes for IPv4 + 2 bytes for port = 6 bytes
                let mut bytes = [0u8; 6];
                bytes[0..4].copy_from_slice(&addr.ip().octets());
                bytes[4..6].copy_from_slice(&addr.port().to_le_bytes());
                self.0.hash(&bytes)
            }
            SocketAddr::V6(addr) => {
                // 16 bytes for IPv6 + 2 bytes for port = 18 bytes
                let mut bytes = [0u8; 18];
                bytes[0..16].copy_from_slice(&addr.ip().octets());
                bytes[16..18].copy_from_slice(&addr.port().to_le_bytes());
                self.0.hash(&bytes)
            }
        }
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
