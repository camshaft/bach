use crate::sim;
use bach::{ext::*, queue, task::yield_now, time::sleep};
use criterion::{Criterion, Throughput};
use std::time::Duration;

pub fn run(c: &mut Criterion) {
    runtime(c);
    queues(c);
    #[cfg(feature = "net")]
    net(c);
}

fn runtime(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime");

    let items = 100_000;

    group.throughput(Throughput::Elements(items));

    group.bench_function("spawn", |b| {
        b.iter(|| {
            sim(|| {
                async move {
                    for i in 0..items {
                        async move { i }.primary().spawn();
                        yield_now().await;
                    }
                }
                .primary()
                .spawn();
            });
        })
    });

    group.bench_function("sleep", |b| {
        b.iter(|| {
            sim(|| {
                async move {
                    for _i in 0..items {
                        sleep(Duration::from_millis(1)).await;
                    }
                }
                .primary()
                .spawn();
            })
        })
    });
}

fn queues(c: &mut Criterion) {
    let mut group = c.benchmark_group("queues");

    let items = 1_000_000;

    group.throughput(Throughput::Elements(items));

    group.bench_function("channel", |b| {
        b.iter(|| {
            sim(|| {
                let (mut sender, mut receiver) =
                    queue::vec_deque::Queue::default().mutex().channel();
                async move {
                    for i in 0..items {
                        sender.send(i).await.unwrap();
                    }
                }
                .primary()
                .spawn();

                async move { while receiver.recv().await.is_ok() {} }
                    .primary()
                    .spawn();
            })
        })
    });
}

#[cfg(feature = "net")]
fn net(c: &mut Criterion) {
    use bach::net::{self, UdpSocket};

    let mut group = c.benchmark_group("net");

    let items = 100_000;

    group.throughput(Throughput::Elements(items));

    group.bench_function("udp", |b| {
        b.iter(|| {
            sim(|| {
                async move {
                    let socket = UdpSocket::bind("client:0").await.unwrap();
                    let server = net::lookup_host("server:1337")
                        .await
                        .unwrap()
                        .next()
                        .unwrap();
                    for i in 0..items {
                        socket.send_to(&i.to_be_bytes(), server).await.unwrap();
                        sleep(Duration::from_millis(2)).await;
                    }
                }
                .primary()
                .group("client")
                .spawn();

                async move {
                    let socket = UdpSocket::bind("server:1337").await.unwrap();
                    let mut buf = items.to_be_bytes();
                    for _i in 0..items {
                        socket.recv_from(&mut buf).await.unwrap();
                    }
                }
                .group("server")
                .spawn();
            })
        })
    });
}
