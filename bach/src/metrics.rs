#[doc(hidden)]
pub mod macro_support {
    pub use ::metrics::*;
    pub use ::tracing::trace;
}

#[macro_export]
macro_rules! measure {
    ($name:literal, $value:expr $(, $key:literal = $v:expr)* $(,)?) => {
        $crate::metrics::macro_support::trace!(measure = %$name, value = ?$value $(, $key = %$v)*);
        $crate::metrics::macro_support::histogram!($name $(, $key => $v)*).record($value);
    };
}

#[macro_export]
macro_rules! count {
    ($name:literal $(, $key:literal = $v:expr)* $(,)?) => {
        $crate::count!($name, 1 $(, $key = $v)*);
    };
    ($name:literal, $value:expr $(, $key:literal = $v:expr)* $(,)?) => {
        $crate::metrics::macro_support::trace!(count = %$name, value = %$value $(, $key = %$v)*);
        $crate::metrics::macro_support::counter!($name $(, $key => $v)*).increment($value);
    };
}
