use crate::executor::{Handle, JoinHandle};
use core::future::Future;

crate::scope::define!(scope, Handle);

pub fn spawn<F: 'static + Future<Output = T> + Send, T: 'static + Send>(
    future: F,
) -> JoinHandle<T> {
    scope::borrow_with(|handle| {
        // try to inherit the parent group
        crate::group::scope::try_borrow_with(|group| {
            if let Some(group) = group {
                handle.spawn(crate::group::Grouped::new(future, *group))
            } else {
                handle.spawn(future)
            }
        })
    })
}

pub mod primary {
    use super::*;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicU64, Ordering};

    pub fn spawn<F: 'static + Future<Output = T> + Send, T: 'static + Send>(
        future: F,
    ) -> JoinHandle<T> {
        super::spawn(create(future))
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

    #[pin_project::pin_project]
    pub struct Wrapped<F> {
        #[pin]
        inner: F,
        guard: Guard,
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
