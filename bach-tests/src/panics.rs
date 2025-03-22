use crate::sim;
use bach::{
    ext::*,
    sync::channel::{Receiver, Sender},
};
use std::{future::poll_fn, task::Poll};

/// Ensures that a task that panics doesn't cause the simulation to double panic
#[test]
#[should_panic = "panic"]
fn task_panic() {
    sim(|| {
        async move {
            panic!("panic");
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

#[test]
#[cfg_attr(feature = "leaks", ignore)]
#[should_panic]
fn max_self_wakes() {
    sim(|| {
        async {
            let () = poll_fn(|cx| {
                cx.waker().wake_by_ref();
                Poll::Pending
            })
            .await;
        }
        .primary()
        .spawn();
    });
}

#[test]
#[cfg_attr(feature = "leaks", ignore)]
#[should_panic]
fn channel_ping_pong() {
    sim(|| {
        let (mut sender_a, receiver_a) = bach::queue::vec_deque::Queue::default().mutex().channel();
        let (sender_b, receiver_b) = bach::queue::vec_deque::Queue::default().mutex().channel();

        let task = |mut send: Sender<()>, mut recv: Receiver<()>| async move {
            loop {
                recv.recv().await.unwrap();
                send.send(()).await.unwrap();
            }
        };

        task(sender_b, receiver_a).primary().spawn();
        task(sender_a.clone(), receiver_b).primary().spawn();

        async move {
            sender_a.send(()).await.unwrap();
        }
        .spawn();
    });
}

#[test]
#[ignore = "TODO this test overflows the stack - need to figure out how to prevent that"]
#[should_panic]
fn spawn_bomb() {
    sim(|| {
        async fn bomb() {
            spawn_bomb();
        }

        fn spawn_bomb() {
            Box::pin(bomb()).primary().spawn();
        }

        spawn_bomb();
    });
}

#[test]
#[should_panic]
#[cfg_attr(feature = "leaks", ignore)]
fn channel_spawn_bomb() {
    sim(|| {
        let (mut sender, mut receiver) = bach::queue::vec_deque::Queue::builder()
            .with_capacity(Some(1))
            .build()
            .mutex()
            .channel();

        async move {
            loop {
                sender.send(()).await.unwrap();
            }
        }
        .primary()
        .spawn();

        async move {
            loop {
                receiver.recv().await.unwrap();
                async {}.spawn();
            }
        }
        .primary()
        .spawn();
    });
}
