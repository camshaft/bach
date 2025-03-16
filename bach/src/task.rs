use crate::executor::{Handle, JoinHandle};
use core::future::Future;

crate::scope::define!(scope, Handle);

pub(crate) mod waker;

pub fn spawn<F, T>(future: F) -> JoinHandle<T>
where
    F: 'static + Future<Output = T> + Send,
    T: 'static + Send,
{
    spawn_named(future, "")
}

pub fn spawn_named<F, N, T>(future: F, name: N) -> JoinHandle<T>
where
    F: 'static + Future<Output = T> + Send,
    N: core::fmt::Display,
    T: 'static + Send,
{
    scope::borrow_with(|handle| {
        // try to inherit the parent group
        crate::group::scope::try_borrow_with(|group| {
            if let Some(group) = group {
                handle.spawn_named(crate::group::Grouped::new(future, *group), name)
            } else {
                handle.spawn_named(future, name)
            }
        })
    })
}

pub mod primary {
    use super::*;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicU64, Ordering};
    use pin_project_lite::pin_project;

    pub fn spawn<F, T>(future: F) -> JoinHandle<T>
    where
        F: 'static + Future<Output = T> + Send,
        T: 'static + Send,
    {
        super::spawn(create(future))
    }

    pub fn spawn_named<F, N, T>(future: F, name: N) -> JoinHandle<T>
    where
        F: 'static + Future<Output = T> + Send,
        N: core::fmt::Display,
        T: 'static + Send,
    {
        super::spawn_named(create(future), name)
    }

    #[derive(Debug)]
    pub struct Guard(Arc<AtomicU64>);

    impl Guard {
        pub(crate) fn new(count: Arc<AtomicU64>) -> Self {
            count.fetch_add(1, Ordering::SeqCst);
            Self(count)
        }
    }

    impl Clone for Guard {
        fn clone(&self) -> Self {
            self.0.fetch_add(1, Ordering::SeqCst);
            Self(self.0.clone())
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    pub fn guard() -> Guard {
        scope::borrow_with(|h| h.primary_guard())
    }

    pub fn create<F: Future>(future: F) -> Wrapped<F> {
        let guard = guard();
        Wrapped {
            inner: future,
            guard,
        }
    }

    pin_project! {
        pub struct Wrapped<F> {
            #[pin]
            inner: F,
            guard: Guard,
        }
    }

    impl<F: Future> Future for Wrapped<F> {
        type Output = F::Output;

        fn poll(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            self.project().inner.poll(cx)
        }
    }
}

pub use info::Info;

pub(crate) mod info {
    use super::*;
    use crate::{
        define,
        tracing::{info_span, Span},
    };
    use pin_project_lite::pin_project;
    use std::sync::Arc;

    define!(scope, Info);

    #[derive(Clone, Debug)]
    pub struct Info {
        id: u64,
        name: Option<Arc<str>>,
    }

    impl Info {
        pub fn current() -> Self {
            scope::borrow_with(|v| v.clone())
        }

        pub fn id(&self) -> u64 {
            self.id
        }

        pub fn name(&self) -> Option<&str> {
            self.name.as_deref()
        }
    }

    pin_project! {
        pub struct WithInfo<F> {
            #[pin]
            inner: F,
            info: Info,
            span: Span,
        }
    }

    impl<F> WithInfo<F> {
        pub fn new(inner: F, id: u64, name: &Arc<str>) -> Self {
            let name = if name.is_empty() {
                None
            } else {
                Some(name.clone())
            };
            let span = if let Some(name) = &name {
                let _ = name;
                info_span!("task", task = ?name)
            } else {
                info_span!("task", task = id)
            };
            let info = Info { id, name };
            Self { inner, info, span }
        }
    }

    impl<F: Future> Future for WithInfo<F> {
        type Output = F::Output;

        fn poll(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            let this = self.project();
            scope::with(this.info.clone(), || {
                this.span.in_scope(|| this.inner.poll(cx))
            })
        }
    }
}
