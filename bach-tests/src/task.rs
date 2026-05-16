use crate::sim;
use bach::{ext::*, task::yield_now};
use std::{future::poll_fn, rc::Rc, task::Poll};

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
fn spawn_non_send_future_and_output() {
    sim(|| {
        let value = Rc::new(123);
        let handle = async move { value }.spawn();

        async move {
            let value = handle.await.unwrap();
            assert_eq!(*value, 123);
        }
        .primary()
        .spawn();
    });
}
