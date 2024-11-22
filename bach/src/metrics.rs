#[doc(hidden)]
#[cfg(feature = "metrics")]
pub mod macro_support {
    pub use ::metrics::*;
}

#[macro_export]
#[cfg(feature = "metrics")]
macro_rules! measure {
    ($name:literal, $value:expr $(, $key:literal = $v:expr)* $(,)?) => {
        $crate::tracing::trace!(measure = %$name, value = ?$value $(, $key = %$v)*);
        $crate::metrics::macro_support::histogram!($name $(, $key => $v)*).record($value);
    };
}

#[macro_export]
#[cfg(not(feature = "metrics"))]
macro_rules! measure {
    ($name:literal, $value:expr $(, $key:literal = $v:expr)* $(,)?) => {
        let _ = $name;
        let _ = $value;
        $(
            let _ = $key;
            let _ = $v;
        )*
    };
}

#[macro_export]
#[cfg(feature = "metrics")]
macro_rules! count {
    ($name:literal $(, $key:literal = $v:expr)* $(,)?) => {
        $crate::count!($name, 1 $(, $key = $v)*);
    };
    ($name:literal, $value:expr $(, $key:literal = $v:expr)* $(,)?) => {
        $crate::tracing::trace!(count = %$name, value = %$value $(, $key = %$v)*);
        $crate::metrics::macro_support::counter!($name $(, $key => $v)*).increment($value);
    };
}

#[macro_export]
#[cfg(not(feature = "metrics"))]
macro_rules! count {
    ($name:literal $(, $key:literal = $v:expr)* $(,)?) => {
        $crate::count!($name, 1 $(, $key = $v)*);
    };
    ($name:literal, $value:expr $(, $key:literal = $v:expr)* $(,)?) => {
        let _ = $name;
        let _ = $value;
        $(
            let _ = $key;
            let _ = $v;
        )*
    }
}
