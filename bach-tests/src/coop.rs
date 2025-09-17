use crate::testing::Log;
use bach::{environment::default::Runtime, ext::*, queue::vec_deque::Queue, sync::channel};
use futures::stream::{FuturesUnordered, StreamExt as _};

fn new_channel<T: 'static + Send>() -> (channel::Sender<T>, channel::Receiver<T>) {
    Queue::builder()
        .with_capacity(Some(10))
        .build()
        .mutex()
        .channel()
}

pub fn sim(f: impl Fn()) -> impl Fn() {
    crate::testing::init_tracing();
    move || {
        let mut rt = Runtime::new().with_coop(true);
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
            let (sender, mut receiver) = new_channel();

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

        let (sender, receiver) = new_channel();

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

#[test]
fn futures_unordered() {
    bolero::check!().exhaustive().run(sim(|| {
        let (accept, mut acceptor) = new_channel::<channel::Sender<()>>();

        async move {
            while let Ok(mut response) = acceptor.recv().await {
                dbg!();
                response.send(()).await.unwrap();
            }
        }
        .spawn();

        async move {
            let mut requests = FuturesUnordered::new();
            let count = 2;

            for idx in 0..count {
                let (send_response, mut read_response) = new_channel::<()>();
                let mut accept = accept.clone();
                requests.push(async move {
                    dbg!(idx);
                    accept.send(send_response).await.unwrap();
                    read_response.recv().await.unwrap();
                    idx
                });
            }

            let mut completed = vec![false; count];

            while !requests.is_empty() {
                if let Some(index) = requests.next().await {
                    dbg!(index);
                    completed[index] = true;
                }
            }

            assert!(completed.iter().all(|v| *v));
        }
        .primary()
        .spawn();
    }));
}
