#[cfg(feature = "tracing")]
pub use tracing::*;

#[cfg(not(feature = "tracing"))]
#[allow(unused_imports)]
mod shim {
    #[derive(Clone, Copy, Debug)]
    pub struct Span(());

    impl Span {
        pub fn disabled() -> Self {
            Self(())
        }

        pub fn in_scope<F: FnOnce() -> R, R>(&self, f: F) -> R {
            f()
        }
    }

    #[macro_export]
    macro_rules! debug_span_ {
        ($($tt:tt)*) => {
            $crate::tracing::Span::disabled()
        };
    }

    pub use crate::debug_span_ as debug_span;

    #[macro_export]
    macro_rules! info_span_ {
        ($($tt:tt)*) => {
            $crate::tracing::Span::disabled()
        };
    }

    pub use crate::info_span_ as info_span;

    #[macro_export]
    macro_rules! trace_ {
        ($($tt:tt)*) => {};
    }

    pub use crate::trace_ as trace;

    pub trait Instrument {
        fn instrument(self, span: Span) -> Self;
    }

    impl<T> Instrument for T {
        fn instrument(self, span: Span) -> Self {
            let _ = span;
            self
        }
    }
}

#[cfg(not(feature = "tracing"))]
pub use shim::*;
