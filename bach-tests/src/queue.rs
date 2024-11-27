use bach::{
    environment::default::Runtime,
    ext::*,
    sync::{duplex::Duplex, queue::vec_deque},
    time::{self, Duration, Instant},
};
use std::sync::atomic::{AtomicU64, Ordering};

fn run(f: impl FnOnce()) -> Duration {
    crate::testing::init_tracing();
    let mut rt = Runtime::new();
    rt.run(f);
    rt.elapsed()
}

type Queue<T> = vec_deque::Queue<T>;

fn duplex<T: 'static + Send + Sync>() -> (Duplex<T>, Duplex<T>) {
    <Duplex<T>>::pair(<Queue<T>>::default(), <Queue<T>>::default())
}

struct AtomicDuration(AtomicU64);

impl AtomicDuration {
    pub const fn new(value: Duration) -> Self {
        Self(AtomicU64::new(value.as_nanos() as _))
    }

    pub fn store(&self, value: Duration, order: Ordering) {
        self.0.store(value.as_nanos() as _, order);
    }

    pub fn load(&self, order: Ordering) -> Duration {
        Duration::from_nanos(self.0.load(order))
    }
}

#[test]
fn request_response() {
    static REQUEST_TIME: AtomicDuration = AtomicDuration::new(Duration::ZERO);
    static RESPONSE_TIME: AtomicDuration = AtomicDuration::new(Duration::ZERO);

    run(|| {
        let (client, server) = duplex::<&'static str>();

        async move {
            client.sender.push("hello").await.unwrap();

            let _response = client.receiver.pop().await.unwrap();
            RESPONSE_TIME.store(Instant::now().elapsed_since_start(), Ordering::Relaxed);
        }
        .primary()
        .spawn_named("client");

        async move {
            time::delay(1.s()).await;
            let _request = server.receiver.pop().await.unwrap();
            REQUEST_TIME.store(Instant::now().elapsed_since_start(), Ordering::Relaxed);

            time::delay(1.s()).await;
            server.sender.push("world").await.unwrap();
        }
        .primary()
        .spawn_named("server");
    });

    assert_eq!(REQUEST_TIME.load(Ordering::Relaxed), 1.s());
    assert_eq!(RESPONSE_TIME.load(Ordering::Relaxed), 2.s());
}

#[test]
fn latent_queue() {
    static RECV_COUNT: AtomicU64 = AtomicU64::new(0);
    const COUNT: u64 = 10;

    let elapsed = run(|| {
        let (sender, receiver) = Queue::builder()
            .with_capacity(Some(20))
            .build()
            .latent(10.ms())
            .sojourn()
            .span("channel")
            .channel();

        async move {
            for idx in 0..COUNT {
                1.ms().sleep().await;
                sender.send(idx).await.unwrap();
            }
        }
        .primary()
        .spawn_named("client");

        async move {
            while let Ok(idx) = receiver.pop().await {
                dbg!(idx);
                RECV_COUNT.fetch_add(1, Ordering::Relaxed);
            }
        }
        .primary()
        .spawn_named("server");
    });

    assert_eq!(elapsed, 20.ms());
    assert_eq!(RECV_COUNT.load(Ordering::Relaxed), COUNT);
}
