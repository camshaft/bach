pub fn init_tracing() {
    #[cfg(feature = "tracing")]
    init_tracing_impl();
}

#[cfg(feature = "tracing")]
fn init_tracing_impl() {
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
                    crate::time::scheduler::scope::try_borrow_mut_with(|s| Some(s.as_ref()?.now()));
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
