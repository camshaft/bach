use crate::testing::Log;
use bach::{environment::default::Runtime, ext::*, queue::vec_deque::Queue};

pub fn sim(f: impl Fn()) -> impl Fn() {
    crate::testing::init_tracing();
    move || {
        let mut rt = Runtime::new().with_coop(true).with_rand(None);
        rt.run(&f);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(dead_code)]
enum Event {
    Start,
    Message {
        receiver: u8,
        sender_group: u8,
        sender_id: u8,
    },
    ReceiverClose {
        receiver: u8,
    },
}

impl crate::testing::Event for Event {
    fn is_start(&self) -> bool {
        matches!(self, Event::Start)
    }
}

#[test]
fn interleavings() {
    static LOG: Log<Event> = Log::new();

    bolero::check!().exhaustive().run(sim(|| {
        LOG.push(Event::Start);

        for group in 0..2 {
            let (sender, mut receiver) = Queue::builder()
                .with_capacity(Some(20))
                .build()
                .mutex()
                .channel();

            async move {
                while let Ok((sender_group, sender_id)) = receiver.pop().await {
                    LOG.push(Event::Message {
                        receiver: group,
                        sender_group,
                        sender_id,
                    });
                }

                LOG.push(Event::ReceiverClose { receiver: group });
            }
            .primary()
            .spawn_named(format!("[{group}] server"));

            for id in 0..2 {
                let mut sender = sender.clone();
                async move {
                    for _ in 0..1 {
                        sender.push((group, id)).await.unwrap();
                    }
                }
                .primary()
                .spawn_named(format!("[{group}] client{id}"));
            }
        }
    }));

    insta::assert_debug_snapshot!(LOG.check());
}

#[test]
fn joined_interleavings() {
    static LOG: Log<Event> = Log::new();

    bolero::check!().exhaustive().run(sim(|| {
        LOG.push(Event::Start);
        eprintln!("start");

        let (sender, receiver) = Queue::builder()
            .with_capacity(Some(20))
            .build()
            .mutex()
            .channel();

        for group in 0..2 {
            let mut receiver = receiver.clone();
            async move {
                while let Ok((sender_group, sender_id)) = receiver.pop().await {
                    LOG.push(Event::Message {
                        receiver: group,
                        sender_group,
                        sender_id,
                    });
                }

                LOG.push(Event::ReceiverClose { receiver: group });
            }
            .primary()
            .spawn_named(format!("[{group}] server"));

            for id in 0..1 {
                let mut sender = sender.clone();
                async move {
                    for _ in 0..1 {
                        sender.push((group, id)).await.unwrap();
                    }
                }
                .primary()
                .spawn_named(format!("[{group}] client{id}"));
            }
        }
    }));

    insta::assert_debug_snapshot!(LOG.check());
}
