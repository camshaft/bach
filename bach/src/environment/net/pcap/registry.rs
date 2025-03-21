use super::Writer;
use crate::group::Group;
use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    fs::{create_dir_all, File},
    io,
    net::IpAddr,
    path::PathBuf,
    sync::Arc,
};

pub struct Registry {
    dir: Option<PathBuf>,
    pcaps: HashMap<Group, State>,
}

impl core::default::Default for Registry {
    fn default() -> Self {
        use std::sync::OnceLock;

        static PCAP_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

        let dir = PCAP_DIR
            .get_or_init(|| {
                let pcap = std::env::var("BACH_PCAP_DIR").ok()?;
                Some(pcap.into())
            })
            .clone();

        let mut registry = Self {
            dir: None,
            pcaps: Default::default(),
        };

        if let Some(mut dir) = dir {
            // the rust test runner uses threads for each test so we can get the test name from that
            if let Some(thread_name) = std::thread::current().name() {
                if !(thread_name.is_empty() || thread_name == "main") {
                    dir.push(thread_name.replace(':', "_"));
                }
            }

            if let Err(err) = registry.set_dir(dir) {
                eprintln!("failed to create pcap directory: {err}");
            }
        }

        registry
    }
}

impl Registry {
    pub fn set_dir<P: Into<PathBuf>>(&mut self, pcap: P) -> io::Result<()> {
        let pcap = pcap.into();
        create_dir_all(&pcap)?;
        self.dir = Some(pcap);
        Ok(())
    }

    pub(crate) fn dns(&mut self, group: &Group, query: &str, ip: &IpAddr) -> bool {
        self.with_entry(group, |state| {
            if state.dns.contains(query) {
                return false;
            }

            let _ = super::dns::write(&mut state.writer, query, ip);

            state.dns.insert(query.to_string());

            true
        })
        .unwrap_or(false)
    }

    pub fn open(&mut self, group: &Group) -> Option<Writer<impl Clone + Send + io::Write>> {
        self.with_entry(group, |state| state.writer.clone())
    }

    fn with_entry<R>(&mut self, group: &Group, f: impl FnOnce(&mut State) -> R) -> Option<R> {
        let dir = self.dir.as_ref()?;
        let name = group.name();
        if name.is_empty() {
            return None;
        }

        match self.pcaps.entry(*group) {
            Entry::Occupied(mut entry) => Some(f(entry.get_mut())),
            Entry::Vacant(entry) => {
                let pcap = dir.join(name).with_extension("pcap");
                let pcap = File::create(pcap).unwrap();
                let pcap = Arc::new(pcap);
                let pcap = Writer::new(pcap).unwrap();
                let state = State {
                    writer: pcap.clone(),
                    dns: Default::default(),
                };
                Some(f(entry.insert(state)))
            }
        }
    }
}

struct State {
    writer: Writer<Arc<File>>,
    dns: HashSet<String>,
}
