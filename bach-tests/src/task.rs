use crate::sim;
use bach::{ext::*, task::yield_now};
use std::{future::poll_fn, task::Poll};

#[test]
fn join() {
    sim(|| {
        let handle = async move { 123 }.spawn();

        async move {
            assert_eq!(handle.await.unwrap(), 123);
        }
        .primary()
        .spawn();
    });
}

#[test]
fn abort_before_complete() {
    sim(|| {
        let handle = async move {
            let mut waker = None;
            let () = poll_fn(|cx| {
                waker = Some(cx.waker().clone());
                Poll::Pending
            })
            .await;
        }
        .spawn();

        async move {
            handle.abort();
            handle.await.unwrap_err();
        }
        .primary()
        .spawn();
    });
}

#[test]
fn abort_after_complete() {
    sim(|| {
        let handle = async move { 123 }.spawn();

        async move {
            handle.abort();
            handle.await.unwrap_err();
        }
        .primary()
        .spawn();
    });
}

#[test]
fn abort_after_yield_complete() {
    sim(|| {
        let handle = async move {
            yield_now().await;
            123
        }
        .spawn();

        async move {
            handle.abort();
            handle.await.unwrap_err();
        }
        .primary()
        .spawn();
    });
}

#[test]
#[should_panic]
fn task_with_no_active_waker() {
    sim(|| {
        std::future::pending::<()>().spawn();
    });
}

#[test]
#[should_panic]
fn retain_waker_and_drop() {
    sim(|| {
        async {
            let mut count = 0;
            let mut waker = None;
            let () = poll_fn(|cx| {
                if count == 0 {
                    waker = Some(cx.waker().clone());
                }
                if count > 10 {
                    waker = None;
                }
                count += 1;
                Poll::Pending
            })
            .await;
        }
        .primary()
        .spawn();
    });
}

#[test]
#[should_panic]
fn self_wake_and_sleep() {
    sim(|| {
        async {
            let mut count = 0;
            let () = poll_fn(|cx| {
                if count < 10 {
                    cx.waker().wake_by_ref();
                }
                count += 1;
                Poll::Pending
            })
            .await;
        }
        .primary()
        .spawn();
    });
}
