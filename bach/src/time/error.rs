use core::fmt;

#[derive(Debug, PartialEq, Eq)]
pub struct Elapsed(());

impl Elapsed {
    pub(crate) fn new() -> Self {
        Elapsed(())
    }
}

impl fmt::Display for Elapsed {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        "deadline has elapsed".fmt(fmt)
    }
}

impl std::error::Error for Elapsed {}

impl From<Elapsed> for std::io::Error {
    fn from(_err: Elapsed) -> std::io::Error {
        std::io::ErrorKind::TimedOut.into()
    }
}

#[cfg(feature = "tokio-compat")]
impl From<tokio::time::error::Elapsed> for Elapsed {
    fn from(_err: tokio::time::error::Elapsed) -> Self {
        Elapsed::new()
    }
}
