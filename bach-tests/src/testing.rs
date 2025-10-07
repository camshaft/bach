use std::{collections::HashMap, sync::Mutex};

pub fn init_tracing() {
    if !cfg!(test) || cfg!(feature = "leaks") {
        return;
    }

    use std::sync::Once;

    static TRACING: Once = Once::new();

    // make sure this only gets initialized once
    TRACING.call_once(|| {
        let format = tracing_subscriber::fmt::format()
            .with_level(false) // don't include levels in formatted output
            .with_timer(Uptime)
            .with_ansi(false)
            .compact(); // Use a less verbose output format.

        struct Uptime;

        // Generate the timestamp from the testing IO provider rather than wall clock.
        impl tracing_subscriber::fmt::time::FormatTime for Uptime {
            fn format_time(
                &self,
                w: &mut tracing_subscriber::fmt::format::Writer<'_>,
            ) -> std::fmt::Result {
                let now =
                    bach::time::scheduler::scope::try_borrow_mut_with(|s| Some(s.as_ref()?.now()));
                if let Some(now) = now {
                    write!(w, "{now}")
                } else {
                    write!(w, "[UNKNOWN]")
                }
            }
        }

        let env_filter = tracing_subscriber::EnvFilter::builder()
            .with_default_directive(tracing::Level::DEBUG.into())
            .with_env_var("BACH_LOG")
            .from_env()
            .unwrap();

        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .event_format(format)
            .with_test_writer()
            .init();
    });
}

pub trait Event: Clone + core::fmt::Debug + Eq + core::hash::Hash {
    fn is_start(&self) -> bool;
}

pub struct Log<T>(Mutex<Vec<T>>);

impl<T: Event> Default for Log<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Event> Log<T> {
    pub const fn new() -> Self {
        Self(Mutex::new(Vec::new()))
    }

    pub fn push(&self, event: T) {
        self.0.lock().unwrap().push(event);
    }

    pub fn check(&self) -> Vec<T> {
        let mut sequences = vec![];
        let mut current_sequence = vec![];
        let mut seen = HashMap::<_, usize>::new();

        let events = core::mem::take(&mut *self.0.lock().unwrap());

        for event in events.iter() {
            if event.is_start() {
                if !current_sequence.is_empty() {
                    let seq = std::mem::take(&mut current_sequence);
                    *seen.entry(seq.clone()).or_default() += 1;
                    sequences.push(seq);
                }
            } else {
                current_sequence.push(event.clone());
            }
        }

        if !current_sequence.is_empty() {
            let seq = std::mem::take(&mut current_sequence);
            *seen.entry(seq.clone()).or_default() += 1;
            sequences.push(seq);
        }

        let mut duplicate = false;

        for (seq, count) in seen {
            if count == 1 {
                continue;
            }
            duplicate = true;
            eprintln!("duplicate sequence ({count}): {seq:#?}");
        }

        assert!(!duplicate, "duplicate interleavings found");
        assert!(!sequences.is_empty(), "no event sequences recorded");

        eprintln!("Found {} unique event sequences", sequences.len());

        events
    }
}
