use std::sync::Mutex;

use bach::{environment::default::Runtime, ext::*, sync::queue::vec_deque::Queue};

fn sim(f: impl Fn()) -> impl Fn() {
    crate::testing::init_tracing();
    move || {
        let mut rt = Runtime::new().with_coop(true).with_rand(None);
        rt.run(&f);
    }
}

#[derive(Debug)]
#[allow(dead_code)]
enum Event {
    Start,
    Message { group: u8, actor: u8 },
}

#[test]
fn interleavings() {
    static LOG: Mutex<Vec<Event>> = Mutex::new(vec![]);

    bolero::check!().exhaustive().run(sim(|| {
        LOG.lock().unwrap().push(Event::Start);

        for group in 0..2 {
            let (sender, receiver) = Queue::builder().with_capacity(Some(20)).build().channel();

            async move {
                while let Ok(actor) = receiver.pop().await {
                    LOG.lock().unwrap().push(Event::Message { group, actor });
                }
            }
            .primary()
            .spawn_named(format!("[{group}] server"));

            for id in 0..2 {
                let sender = sender.clone();
                async move {
                    for _ in 0..1 {
                        sender.push(id).await.unwrap();
                    }
                }
                .primary()
                .spawn_named(format!("[{group}] client{id}"));
            }
        }
    }));

    insta::assert_debug_snapshot!(LOG.lock().unwrap());
}
