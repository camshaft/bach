use std::{
    collections::{hash_map::Entry, HashMap},
    io,
    sync::{Arc, Mutex},
};

const EPHEMERAL_PORT_START: u16 = 49152;

pub struct Allocator {
    active_ports: PortMap,
    next: u16,
}

impl Default for Allocator {
    fn default() -> Self {
        Self {
            active_ports: Default::default(),
            next: EPHEMERAL_PORT_START,
        }
    }
}

impl Allocator {
    pub fn reserve(&self, port: u16, reuse_port: bool) -> io::Result<Reservation> {
        let mut active = self.active_ports.lock().unwrap();

        match active.entry(port) {
            Entry::Occupied(entry) if reuse_port => {
                *entry.into_mut() += 1;
            }
            Entry::Occupied(_entry) => {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    "Port is already in use",
                ));
            }
            Entry::Vacant(entry) => {
                entry.insert(1);
            }
        }

        drop(active);

        Ok(Reservation {
            active_ports: self.active_ports.clone(),
            port,
        })
    }

    pub fn ephemeral(&mut self) -> io::Result<Reservation> {
        let mut iterations = 0;
        let mut active = self.active_ports.lock().unwrap();

        loop {
            let port = self.next;

            self.next = port.wrapping_add(1).max(EPHEMERAL_PORT_START);

            if active.contains_key(&port) {
                iterations += 1;
                if iterations > u16::MAX - EPHEMERAL_PORT_START {
                    break;
                }
                continue;
            }

            *active.entry(port).or_default() += 1;
            drop(active);

            return Ok(Reservation {
                active_ports: self.active_ports.clone(),
                port,
            });
        }

        Err(io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            "No more ephemeral ports available",
        ))
    }
}

type PortMap = Arc<Mutex<HashMap<u16, u16>>>;

pub struct Reservation {
    active_ports: PortMap,
    port: u16,
}

impl Reservation {
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if let Ok(mut ports) = self.active_ports.lock() {
            if let Entry::Occupied(mut entry) = ports.entry(self.port) {
                *entry.get_mut() -= 1;
                if *entry.get() == 0 {
                    entry.remove();
                }
            }
        }
    }
}
