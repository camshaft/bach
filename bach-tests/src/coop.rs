use bach::{environment::default::Runtime, ext::*, sync::queue::vec_deque::Queue};
use std::{collections::HashMap, sync::Mutex};

fn sim(f: impl Fn()) -> impl Fn() {
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

impl Event {
    fn check(v: &[Event]) -> &[Event] {
        let mut group = vec![];
        let mut seen = HashMap::<_, usize>::new();
        for event in v {
            match *event {
                Event::Start => {
                    let group = std::mem::take(&mut group);
                    *seen.entry(group).or_default() += 1;
                }
                _ => {
                    group.push(*event);
                }
            }
        }
        *seen.entry(group).or_default() += 1;

        let mut duplicate = false;

        for (group, count) in seen {
            if count == 1 {
                continue;
            }
            duplicate = true;
            eprintln!("duplicate ({count}): {group:#?}");
        }

        assert!(!duplicate, "duplicate interleavings found");

        v
    }
}

#[test]
fn interleavings() {
    static LOG: Mutex<Vec<Event>> = Mutex::new(vec![]);

    bolero::check!().exhaustive().run(sim(|| {
        LOG.lock().unwrap().push(Event::Start);

        for group in 0..2 {
            let (sender, receiver) = Queue::builder().with_capacity(Some(20)).build().channel();

            async move {
                while let Ok((sender_group, sender_id)) = receiver.pop().await {
                    LOG.lock().unwrap().push(Event::Message {
                        receiver: group,
                        sender_group,
                        sender_id,
                    });
                }

                LOG.lock()
                    .unwrap()
                    .push(Event::ReceiverClose { receiver: group });
            }
            .primary()
            .spawn_named(format!("[{group}] server"));

            for id in 0..2 {
                let sender = sender.clone();
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

    insta::assert_debug_snapshot!(Event::check(&*LOG.lock().unwrap()));
}

#[test]
fn joined_interleavings() {
    static LOG: Mutex<Vec<Event>> = Mutex::new(vec![]);

    bolero::check!().exhaustive().run(sim(|| {
        LOG.lock().unwrap().push(Event::Start);
        eprintln!("start");

        let (sender, receiver) = Queue::builder().with_capacity(Some(20)).build().channel();

        for group in 0..2 {
            let receiver = receiver.clone();
            async move {
                while let Ok((sender_group, sender_id)) = receiver.pop().await {
                    LOG.lock().unwrap().push(Event::Message {
                        receiver: group,
                        sender_group,
                        sender_id,
                    });
                }

                LOG.lock()
                    .unwrap()
                    .push(Event::ReceiverClose { receiver: group });
            }
            .primary()
            .spawn_named(format!("[{group}] server"));

            for id in 0..1 {
                let sender = sender.clone();
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

    insta::assert_debug_snapshot!(Event::check(&*LOG.lock().unwrap()));
}
