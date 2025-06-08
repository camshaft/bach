use super::*;
use crate::testing::Log;
use bach::sync::mpsc::{channel, unbounded_channel};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Event {
    Start,
    SendStarted { task: usize },
    SendCompleted { task: usize },
    RecvStarted { task: usize },
    RecvCompleted { task: usize, value: Option<i32> },
}

impl crate::testing::Event for Event {
    fn is_start(&self) -> bool {
        matches!(self, Event::Start)
    }
}

#[test]
fn mpsc_try_operations() {
    bolero::check!().exhaustive().run(sim(|| {
        async {
            let (tx, mut rx) = channel(2);

            // Try sending messages
            tx.try_send(1).unwrap();
            tx.try_send(2).unwrap();

            // Channel should be full now
            match tx.try_send(3) {
                Ok(_) => panic!("Channel should be full"),
                Err(e) => match e {
                    bach::sync::mpsc::error::TrySendError::Full(_) => {} // expected
                    _ => panic!("Unexpected error: {e:?}"),
                },
            }

            // Try receiving messages
            assert_eq!(rx.try_recv().unwrap(), 1);
            assert_eq!(rx.try_recv().unwrap(), 2);

            // Channel should be empty now
            assert!(rx.try_recv().is_err());
        }
        .primary()
        .spawn();
    }));
}

#[test]
fn mpsc_send_recv() {
    static LOG: Log<Event> = Log::new();

    bolero::check!().exhaustive().run(sim(|| {
        LOG.push(Event::Start);

        let (tx, mut rx) = channel(1);

        // Spawn a task that sends two values (will block on the second one)
        async move {
            LOG.push(Event::SendStarted { task: 1 });
            tx.send(42).await.unwrap();
            LOG.push(Event::SendCompleted { task: 1 });

            LOG.push(Event::SendStarted { task: 1 });
            tx.send(43).await.unwrap();
            LOG.push(Event::SendCompleted { task: 1 });
        }
        .primary()
        .spawn();

        // Spawn a task that receives values
        async move {
            LOG.push(Event::RecvStarted { task: 2 });
            let val = rx.recv().await;
            LOG.push(Event::RecvCompleted {
                task: 2,
                value: val,
            });

            LOG.push(Event::RecvStarted { task: 2 });
            let val = rx.recv().await;
            LOG.push(Event::RecvCompleted {
                task: 2,
                value: val,
            });
        }
        .primary()
        .spawn();
    }));

    insta::assert_debug_snapshot!(LOG.check());
}

#[test]
fn mpsc_multiple_senders() {
    static LOG: Log<Event> = Log::new();

    bolero::check!().exhaustive().run(sim(|| {
        LOG.push(Event::Start);

        let (tx, mut rx) = channel(2);

        // First sender task
        for task in 1..=2 {
            let tx = tx.clone();
            async move {
                LOG.push(Event::SendStarted { task });
                tx.send((task * 10) as _).await.unwrap();
                LOG.push(Event::SendCompleted { task });
            }
            .primary()
            .spawn_named(format!("Sender {task}"));
        }

        // Receiver task
        async move {
            LOG.push(Event::RecvStarted { task: 3 });
            let val = rx.recv().await;
            LOG.push(Event::RecvCompleted {
                task: 3,
                value: val,
            });

            LOG.push(Event::RecvStarted { task: 3 });
            let val = rx.recv().await;
            LOG.push(Event::RecvCompleted {
                task: 3,
                value: val,
            });
        }
        .primary()
        .spawn_named("Receiver");
    }));

    insta::assert_debug_snapshot!(LOG.check());
}

#[test]
fn mpsc_channel_closing() {
    static LOG: Log<Event> = Log::new();

    bolero::check!().exhaustive().run(sim(|| {
        LOG.push(Event::Start);

        let (tx, mut rx) = channel::<i32>(1);

        // Task to close the sender
        async move {
            // Let's send one message first
            LOG.push(Event::SendStarted { task: 1 });
            tx.send(100).await.unwrap();
            LOG.push(Event::SendCompleted { task: 1 });

            // Close by dropping
            drop(tx);
        }
        .primary()
        .spawn_named("Sender");

        // Receiver task that continues after channel is closed
        async move {
            // First message should come through
            LOG.push(Event::RecvStarted { task: 2 });
            let val = rx.recv().await;
            LOG.push(Event::RecvCompleted {
                task: 2,
                value: val,
            });

            // Second receive should get None (channel closed)
            LOG.push(Event::RecvStarted { task: 2 });
            let val = rx.recv().await;
            LOG.push(Event::RecvCompleted {
                task: 2,
                value: val,
            });
        }
        .primary()
        .spawn_named("Receiver");
    }));

    insta::assert_debug_snapshot!(LOG.check());
}

#[test]
fn mpsc_permit_operations() {
    static LOG: Log<Event> = Log::new();

    bolero::check!().exhaustive().run(sim(|| {
        LOG.push(Event::Start);

        let (tx, mut rx) = channel(1);

        // Task to send using permits
        async move {
            LOG.push(Event::SendStarted { task: 1 });
            let permit = tx.reserve().await.unwrap();
            permit.send(50);
            LOG.push(Event::SendCompleted { task: 1 });
        }
        .primary()
        .spawn_named("Sender");

        // Receiver task
        async move {
            LOG.push(Event::RecvStarted { task: 2 });
            let val = rx.recv().await;
            LOG.push(Event::RecvCompleted {
                task: 2,
                value: val,
            });
        }
        .primary()
        .spawn_named("Receiver");
    }));

    insta::assert_debug_snapshot!(LOG.check());
}

#[test]
fn mpsc_unbounded_channel_operations() {
    static LOG: Log<Event> = Log::new();

    bolero::check!().exhaustive().run(sim(|| {
        LOG.push(Event::Start);

        let (tx, mut rx) = unbounded_channel::<i32>();

        // Task to send multiple messages without blocking
        async move {
            for i in 0..5 {
                LOG.push(Event::SendStarted { task: 1 });
                tx.send(i).unwrap();
                LOG.push(Event::SendCompleted { task: 1 });
            }
        }
        .primary()
        .spawn_named("Sender");

        // Receiver task
        async move {
            for _ in 0..5 {
                LOG.push(Event::RecvStarted { task: 2 });
                let val = rx.recv().await;
                LOG.push(Event::RecvCompleted {
                    task: 2,
                    value: val,
                });
            }
        }
        .primary()
        .spawn_named("Receiver");
    }));

    insta::assert_debug_snapshot!(LOG.check());
}

#[test]
fn mpsc_weak_sender_operations() {
    static LOG: Log<Event> = Log::new();

    bolero::check!().exhaustive().run(sim(|| {
        LOG.push(Event::Start);

        let (tx, mut rx) = channel::<i32>(1);

        // Create a weak sender but DON'T drop the original sender yet
        let weak_tx = tx.downgrade();

        // Task to upgrade and use weak sender
        async move {
            LOG.push(Event::SendStarted { task: 1 });
            if let Some(tx) = weak_tx.upgrade() {
                tx.send(75).await.unwrap();
                LOG.push(Event::SendCompleted { task: 1 });
            } else {
                panic!("Failed to upgrade weak sender");
            }

            // A sender that does not prevent the channel from being closed.
            //
            // If all Sender instances of a channel were dropped and only WeakSender
            // instances remain, the channel is closed.
            drop(tx);

            assert!(weak_tx.upgrade().is_none());
        }
        .primary()
        .spawn_named("Weak Sender");

        // Receiver task
        async move {
            LOG.push(Event::RecvStarted { task: 2 });
            let val = rx.recv().await;
            LOG.push(Event::RecvCompleted {
                task: 2,
                value: val,
            });
        }
        .primary()
        .spawn_named("Receiver");
    }));

    insta::assert_debug_snapshot!(LOG.check());
}

#[test]
fn mpsc_reserve_many() {
    static LOG: Log<Event> = Log::new();

    bolero::check!().exhaustive().run(sim(|| {
        LOG.push(Event::Start);

        let (tx, mut rx) = channel(3);

        // Task to reserve multiple slots
        async move {
            LOG.push(Event::SendStarted { task: 1 });
            let permits = tx.reserve_many(2).await.unwrap();

            let mut count = 0;
            for permit in permits {
                permit.send(count);
                count += 1;
            }
            LOG.push(Event::SendCompleted { task: 1 });
        }
        .primary()
        .spawn_named("Multi-Sender");

        // Receiver task
        async move {
            for _ in 0..2 {
                LOG.push(Event::RecvStarted { task: 2 });
                let val = rx.recv().await;
                LOG.push(Event::RecvCompleted {
                    task: 2,
                    value: val,
                });
            }
        }
        .primary()
        .spawn_named("Receiver");
    }));

    insta::assert_debug_snapshot!(LOG.check());
}
