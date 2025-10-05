use crate::executor::Handle;
use core::{
    future::{poll_fn, Future},
    task::Poll,
};

crate::scope::define!(scope, Handle);

mod join;
pub(crate) mod spawn;
pub(crate) mod supervisor;
pub(crate) mod waker;

pub use join::{JoinError, JoinHandle};

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
    scope::borrow_with(|handle| handle.spawn_named(future, name))
}

pub async fn yield_now() {
    let mut pending = true;
    poll_fn(|cx| {
        if core::mem::take(&mut pending) {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        Poll::Ready(())
    })
    .await
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
        group::Group,
        task::supervisor::RunOutcome,
        tracing::{info_span, Span},
    };
    use pin_project_lite::pin_project;
    use std::{sync::Arc, task::ready};

    define!(scope, Info);

    #[derive(Clone, Debug)]
    pub struct Info {
        id: u64,
        pub(crate) group: Group,
        name: Option<Arc<str>>,
        paid_watermark: u64,
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

        pub(crate) fn debt(&self) -> u64 {
            self.group
                .tick_watermark()
                .saturating_sub(self.paid_watermark)
        }

        pub(crate) fn record_cost(&self, cost: core::time::Duration) {
            let ticks = crate::time::resolution::duration_to_ticks(cost);
            self.group.with_tick_watermark(|watermark| {
                if *watermark < self.paid_watermark {
                    *watermark = self.paid_watermark;
                }
                *watermark += ticks;
            });
        }
    }

    pin_project! {
        pub struct WithInfo<F> {
            #[pin]
            inner: F,
            info: Info,
            span: Span,
            #[pin]
            debt: Debt,
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
            let group = Group::current();
            // inherit the paid cost from the parent task
            let paid_watermark =
                scope::try_borrow_with(|parent| parent.as_ref().map(|p| p.paid_watermark))
                    .unwrap_or_else(|| group.tick_watermark());
            let info = Info {
                id,
                name,
                group,
                paid_watermark,
            };
            let debt = Debt { timer: None };
            Self {
                inner,
                info,
                span,
                debt,
            }
        }

        pub(crate) fn record_cost(self: core::pin::Pin<&mut Self>, cost: core::time::Duration) {
            self.project().info.record_cost(cost);
        }

        pub(crate) fn poll(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> RunOutcome<F::Output>
        where
            F: Future,
        {
            let this = self.project();
            match this
                .debt
                .poll(&this.info.group, &mut this.info.paid_watermark, cx)
            {
                Poll::Pending => return RunOutcome::PayingDebt,
                Poll::Ready(()) => {}
            }
            let (info, res) = scope::with(this.info.clone(), || {
                this.span.in_scope(|| this.inner.poll(cx))
            });
            *this.info = info;
            match res {
                Poll::Pending => RunOutcome::ExecutedApplication,
                Poll::Ready(output) => RunOutcome::Done(output),
            }
        }
    }

    pin_project! {
        struct Debt {
            #[pin]
            timer: Option<crate::time::Sleep>,
        }
    }

    impl Debt {
        fn poll(
            self: std::pin::Pin<&mut Self>,
            group: &Group,
            paid_watermark: &mut u64,
            cx: &mut std::task::Context<'_>,
        ) -> Poll<()> {
            let mut this = self.project();
            loop {
                if let Some(timer) = this.timer.as_mut().as_pin_mut() {
                    ready!(timer.poll(cx));
                    this.timer.set(None);
                }

                // Update what the task has paid
                *paid_watermark = crate::time::scheduler::ticks();

                let group_tick_watermark = group.tick_watermark();
                if group_tick_watermark <= *paid_watermark {
                    return Poll::Ready(());
                }
                let timer = crate::time::sleep_until_tick(group_tick_watermark);
                this.timer.as_mut().set(Some(timer));
            }
        }
    }
}
