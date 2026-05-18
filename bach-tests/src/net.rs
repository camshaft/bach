use crate::sim;
use bach::{
    ext::*,
    net::{lookup_host, socket::SendOptions, UdpSocket},
};
use std::{io::IoSlice, time::Duration};
use tracing::info;

fn udp_ping_pong() {
    info!("start");

    for i in 0..2 {
        async move {
            let socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();
            info!(local_addr = %socket.local_addr().unwrap(), "opened socket");

            socket.send_to(b"ping", "server:8080").await.unwrap();
            info!("sent ping");

            let mut data = [0; 4];
            let (len, addr) = socket.recv_from(&mut data).await.unwrap();

            info!(msg = ?data[..len], %addr, "received message");

            assert_eq!(&data[..len], b"pong");

            info!("close");
        }
        .group(format!("client_{i}"))
        .primary()
        .spawn();
    }

    async {
        let socket = UdpSocket::bind("server:8080").await.unwrap();
        info!(local_addr = %socket.local_addr().unwrap(), "opened socket");

        loop {
            let mut data = [0; 4];
            let (len, addr) = socket.recv_from(&mut data).await.unwrap();

            info!(msg = ?data[..len], %addr, "received message");

            assert_eq!(&data[..len], b"ping");

            socket.send_to(b"pong", addr).await.unwrap();

            info!("sent pong");
        }
    }
    .group("server")
    .spawn();
}

#[test]
fn udp_ping_pong_test() {
    sim(udp_ping_pong);
}

#[test]
#[cfg(feature = "coop")]
fn coop_udp_ping_pong_test() {
    bolero::check!()
        .exhaustive()
        .run(crate::coop::sim(udp_ping_pong))
}

#[test]
fn multiple_sockets() {
    sim(|| {
        async {
            let socket1 = UdpSocket::bind("0.0.0.0:0").await.unwrap();
            let socket2 = UdpSocket::bind("0.0.0.0:0").await.unwrap();

            assert_ne!(socket1.local_addr().unwrap(), socket2.local_addr().unwrap());
        }
        .group("client")
        .spawn();
    });
}

#[test]
fn gso() {
    static BUFFER: &[u8] = b"0123456789";

    sim(|| {
        async move {
            let socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();
            info!(local_addr = %socket.local_addr().unwrap(), "opened socket");

            let mut opts = SendOptions::default();
            opts.segment_len = Some(1);

            socket
                .send_msg("server:8080", &[IoSlice::new(BUFFER)], opts)
                .await
                .unwrap();

            for expected in BUFFER {
                let mut data = [0; 1];
                let (len, addr) = socket.recv_from(&mut data).await.unwrap();

                info!(msg = ?data[..len], %addr, "received message");

                assert_eq!(data[0], *expected);
            }
        }
        .group("client")
        .primary()
        .spawn();

        async {
            let socket = UdpSocket::bind("server:8080").await.unwrap();
            info!(local_addr = %socket.local_addr().unwrap(), "opened socket");

            for expected in BUFFER {
                let mut data = [0; 4];
                let (len, addr) = socket.recv_from(&mut data).await.unwrap();

                info!(msg = ?data[..len], %addr, "received message");
                assert_eq!(data[0], *expected);

                socket.send_to(&data[..len], addr).await.unwrap();
            }
        }
        .group("server")
        .spawn();
    });
}

#[test]
fn gro() {
    const BUFFER: &[u8] = b"0123456789";
    const SEGMENT_LEN: usize = 2;

    sim(|| {
        async move {
            let socket = UdpSocket::bind("client:9090").await.unwrap();

            // Wait past network latency so all segments are in the receive buffer
            bach::time::sleep(Duration::from_millis(100)).await;

            // Split the receive buffer into two slices so that coalesced segments
            // cross the slice boundary (e.g. segment "45" spans data1[4] and data2[0]).
            let mut data1 = [0u8; BUFFER.len() / 2];
            let mut data2 = [0u8; BUFFER.len() / 2];
            let mut opts = bach::net::socket::RecvOptions::default();
            opts.gro = true;
            let res = socket
                .recv_msg(
                    &mut [
                        std::io::IoSliceMut::new(&mut data1),
                        std::io::IoSliceMut::new(&mut data2),
                    ],
                    opts,
                )
                .await
                .unwrap();

            assert_eq!(res.segment_len, SEGMENT_LEN, "segment_len mismatch");
            assert_eq!(res.len, BUFFER.len(), "total received length mismatch");
            let combined: Vec<u8> = data1.iter().chain(data2.iter()).copied().collect();
            assert_eq!(&combined[..res.len], BUFFER, "payload mismatch");
        }
        .group("client")
        .primary()
        .spawn();

        async move {
            let socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();
            let mut opts = SendOptions::default();
            opts.segment_len = Some(SEGMENT_LEN);
            socket
                .send_msg("client:9090", &[IoSlice::new(BUFFER)], opts)
                .await
                .unwrap();
        }
        .group("server")
        .spawn();
    });
}

#[test]
fn gro_undersized_tail() {
    // 9 bytes split into 3-byte segments: "012", "345", "678" — followed by a
    // 1-byte tail "9" that is smaller than the segment size.  GRO must coalesce
    // the three uniform segments and leave the undersized tail for the next recv.
    const SEGMENT_LEN: usize = 3;
    const FULL: &[u8] = b"012345678";
    const TAIL: &[u8] = b"9";

    sim(|| {
        async move {
            let socket = UdpSocket::bind("client:9090").await.unwrap();

            // Wait past network latency so all packets are in the receive buffer
            bach::time::sleep(Duration::from_millis(100)).await;

            let mut data = [0u8; 16];
            let mut opts = bach::net::socket::RecvOptions::default();
            opts.gro = true;
            let res = socket
                .recv_msg(&mut [std::io::IoSliceMut::new(&mut data)], opts)
                .await
                .unwrap();

            // The three full segments are coalesced; the undersized tail is held back
            assert_eq!(res.segment_len, SEGMENT_LEN, "segment_len mismatch");
            assert_eq!(res.len, FULL.len(), "coalesced length mismatch");
            assert_eq!(&data[..res.len], FULL, "coalesced payload mismatch");

            // The pending undersized tail is returned by the next recv
            let (len, _) = socket.recv_from(&mut data).await.unwrap();
            assert_eq!(len, TAIL.len(), "tail length mismatch");
            assert_eq!(&data[..len], TAIL, "tail payload mismatch");
        }
        .group("client")
        .primary()
        .spawn();

        async move {
            let socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();
            // Send the full segments via GSO
            let mut opts = SendOptions::default();
            opts.segment_len = Some(SEGMENT_LEN);
            socket
                .send_msg("client:9090", &[IoSlice::new(FULL)], opts)
                .await
                .unwrap();
            // Then send the undersized tail as a separate smaller packet
            socket.send_to(TAIL, "client:9090").await.unwrap();
        }
        .group("server")
        .spawn();
    });
}

#[test]
fn udp_unidirectional() {
    let items: u64 = if cfg!(feature = "leaks") { 500 } else { 10_000 };
    sim(|| {
        async move {
            let socket = UdpSocket::bind("client:0").await.unwrap();
            let server = lookup_host("server:1337").await.unwrap().next().unwrap();
            for i in 0..items {
                socket.send_to(&i.to_be_bytes(), server).await.unwrap();
                // pace packets so there's no loss
                bach::time::sleep(Duration::from_millis(1)).await;
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
        .primary()
        .group("server")
        .spawn();
    });
}

#[cfg(feature = "net-monitor")]
mod monitors {
    use super::*;
    use crate::sim;
    use bach::{environment::default::Runtime, net::monitor};
    use std::{
        io,
        sync::atomic::{AtomicU64, AtomicUsize, Ordering},
    };

    struct AtomicDuration(AtomicU64);

    impl AtomicDuration {
        const fn new(value: Duration) -> Self {
            Self(AtomicU64::new(value.as_nanos() as _))
        }

        fn store(&self, value: Duration, order: Ordering) {
            self.0.store(value.as_nanos() as _, order);
        }

        fn load(&self, order: Ordering) -> Duration {
            Duration::from_nanos(self.0.load(order))
        }
    }

    #[test]
    fn packet_sent_counter() {
        static COUNT: AtomicUsize = AtomicUsize::new(0);

        sim(|| {
            monitor::on_packet_sent(|packet| {
                info!(?packet, "packet_sent");
                COUNT.fetch_add(1, Ordering::Relaxed);
                Default::default()
            });

            udp_ping_pong();
        });

        assert_eq!(COUNT.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn socket_read_write_counter() {
        static W_COUNT: AtomicUsize = AtomicUsize::new(0);
        static R_COUNT: AtomicUsize = AtomicUsize::new(0);

        sim(|| {
            monitor::on_socket_write(|write| {
                info!(?write, "socket_write");
                W_COUNT.fetch_add(1, Ordering::Relaxed);
                Ok(())
            });
            monitor::on_socket_read(|read| {
                info!(?read, "socket_read");
                R_COUNT.fetch_add(1, Ordering::Relaxed);
                Ok(())
            });

            udp_ping_pong();
        });

        assert_eq!(W_COUNT.load(Ordering::Relaxed), 4);
        assert_eq!(R_COUNT.load(Ordering::Relaxed), 4);
    }

    #[test]
    #[should_panic = "SOCKET_WRITE_FAIL"]
    fn socket_write_fail_counter() {
        sim(|| {
            monitor::on_socket_write(|write| {
                info!(?write, "socket_write");
                Err(io::Error::other("SOCKET_WRITE_FAIL"))
            });

            udp_ping_pong();
        });
    }

    #[test]
    #[should_panic = "SOCKET_READ_FAIL"]
    fn socket_read_fail_counter() {
        sim(|| {
            monitor::on_socket_read(|read| {
                info!(?read, "socket_read");
                Err(io::Error::other("SOCKET_READ_FAIL"))
            });

            udp_ping_pong();
        });
    }

    #[test]
    #[should_panic = "PACKET_SENT"]
    fn packet_monitor_panic() {
        sim(|| {
            monitor::on_packet_sent(|_| {
                panic!("PACKET_SENT");
            });

            udp_ping_pong();
        });
    }

    #[test]
    fn packet_send_delay_is_relative_to_network_base() {
        let startup = Duration::from_nanos(1);

        crate::testing::init_tracing();

        let mut rt = Runtime::new();
        rt.run(|| {
            monitor::on_packet_sent(|_| monitor::delay(5.ms()).into());

            async {
                let socket = UdpSocket::bind("server:8080").await.unwrap();
                let mut data = [0; 4];
                let (len, _) = socket.recv_from(&mut data).await.unwrap();
                assert_eq!(&data[..len], b"ping");
            }
            .group("server")
            .primary()
            .spawn();

            async move {
                bach::time::sleep(startup).await;
                let socket = UdpSocket::bind("client:0").await.unwrap();
                socket.send_to(b"ping", "server:8080").await.unwrap();
            }
            .group("client")
            .primary()
            .spawn();
        });

        assert_eq!(rt.elapsed(), startup + 55.ms());
    }

    #[test]
    fn packet_receive_delay_is_relative_to_network_base() {
        let startup = Duration::from_nanos(1);

        crate::testing::init_tracing();

        let mut rt = Runtime::new();
        rt.run(|| {
            monitor::on_packet(|_, operation| match operation {
                monitor::Operation::Receive => monitor::delay(5.ms()).into(),
                monitor::Operation::Send => Default::default(),
            });

            async {
                let socket = UdpSocket::bind("server:8080").await.unwrap();
                let mut data = [0; 4];
                let (len, _) = socket.recv_from(&mut data).await.unwrap();
                assert_eq!(&data[..len], b"ping");
            }
            .group("server")
            .primary()
            .spawn();

            async move {
                bach::time::sleep(startup).await;
                let socket = UdpSocket::bind("client:0").await.unwrap();
                socket.send_to(b"ping", "server:8080").await.unwrap();
            }
            .group("client")
            .primary()
            .spawn();
        });

        assert_eq!(rt.elapsed(), startup + 55.ms());
    }

    #[test]
    fn packet_duplication_uses_count_and_duplicate_context() {
        static ORIGINAL_SENDS: AtomicUsize = AtomicUsize::new(0);
        static DUPLICATE_SENDS: AtomicUsize = AtomicUsize::new(0);
        static LAST_RECEIVE: AtomicDuration = AtomicDuration::new(Duration::ZERO);
        let startup = Duration::from_nanos(1);

        crate::testing::init_tracing();
        ORIGINAL_SENDS.store(0, Ordering::Relaxed);
        DUPLICATE_SENDS.store(0, Ordering::Relaxed);
        LAST_RECEIVE.store(Duration::ZERO, Ordering::Relaxed);

        let mut rt = Runtime::new();
        rt.run(|| {
            monitor::on_packet(|packet, operation| {
                if operation == monitor::Operation::Send {
                    if packet.is_duplicate {
                        DUPLICATE_SENDS.fetch_add(1, Ordering::Relaxed);
                        return monitor::delay(5.ms()).into();
                    } else {
                        ORIGINAL_SENDS.fetch_add(1, Ordering::Relaxed);
                        return monitor::duplicate(2).into();
                    }
                }

                Default::default()
            });

            async {
                let socket = UdpSocket::bind("server:8080").await.unwrap();
                let mut data = [0; 4];
                for _ in 0..3 {
                    let (len, _) = socket.recv_from(&mut data).await.unwrap();
                    assert_eq!(&data[..len], b"ping");
                }
                LAST_RECEIVE.store(
                    bach::time::Instant::now().elapsed_since_start(),
                    Ordering::Relaxed,
                );
            }
            .group("server")
            .primary()
            .spawn();

            async move {
                bach::time::sleep(startup).await;
                let socket = UdpSocket::bind("client:0").await.unwrap();
                socket.send_to(b"ping", "server:8080").await.unwrap();
            }
            .group("client")
            .primary()
            .spawn();
        });

        assert_eq!(ORIGINAL_SENDS.load(Ordering::Relaxed), 1);
        assert_eq!(DUPLICATE_SENDS.load(Ordering::Relaxed), 2);
        assert_eq!(LAST_RECEIVE.load(Ordering::Relaxed), startup + 55.ms());
        assert_eq!(rt.elapsed(), startup + 55.ms());
    }

    #[test]
    fn duplicate_delay_is_relative_to_send_time_not_original_delay() {
        static ORIGINAL_RECEIVE: AtomicDuration = AtomicDuration::new(Duration::ZERO);
        static DUPLICATE_RECEIVE: AtomicDuration = AtomicDuration::new(Duration::ZERO);
        let startup = Duration::from_nanos(1);

        crate::testing::init_tracing();
        ORIGINAL_RECEIVE.store(Duration::ZERO, Ordering::Relaxed);
        DUPLICATE_RECEIVE.store(Duration::ZERO, Ordering::Relaxed);

        let mut rt = Runtime::new();
        rt.run(|| {
            // Original gets 20ms delay, duplicate gets 5ms delay from monitors.
            // With Absolute offset (default), duplicate delay is independent of original.
            // Duplicate arrives at: base + 5ms = 55ms
            // Original arrives at: base + 20ms = 70ms
            monitor::on_packet(|packet, operation| {
                if operation == monitor::Operation::Send {
                    if packet.is_duplicate {
                        return monitor::delay(5.ms()).into();
                    }
                    return monitor::delay(20.ms()).into();
                }
                Default::default()
            });
            monitor::on_packet(|packet, operation| {
                if operation == monitor::Operation::Send && !packet.is_duplicate {
                    return monitor::duplicate(1).absolute().into();
                }
                Default::default()
            });

            async {
                let socket = UdpSocket::bind("server:8080").await.unwrap();
                let mut data = [0; 4];

                // Receive both packets and record arrival times
                let (len, _) = socket.recv_from(&mut data).await.unwrap();
                assert_eq!(&data[..len], b"ping");
                let first = bach::time::Instant::now().elapsed_since_start();

                let (len, _) = socket.recv_from(&mut data).await.unwrap();
                assert_eq!(&data[..len], b"ping");
                let second = bach::time::Instant::now().elapsed_since_start();

                // Duplicate (5ms + 50ms base) arrives before original (20ms + 50ms base)
                DUPLICATE_RECEIVE.store(first, Ordering::Relaxed);
                ORIGINAL_RECEIVE.store(second, Ordering::Relaxed);
            }
            .group("server")
            .primary()
            .spawn();

            async move {
                bach::time::sleep(startup).await;
                let socket = UdpSocket::bind("client:0").await.unwrap();
                socket.send_to(b"ping", "server:8080").await.unwrap();
            }
            .group("client")
            .primary()
            .spawn();
        });

        // base latency = 50ms
        // Duplicate: send_time + 5ms + 50ms base = startup + 55ms
        // Original: send_time + 20ms + 50ms base = startup + 70ms
        assert_eq!(DUPLICATE_RECEIVE.load(Ordering::Relaxed), startup + 55.ms());
        assert_eq!(ORIGINAL_RECEIVE.load(Ordering::Relaxed), startup + 70.ms());
    }

    #[test]
    fn relative_duplicate_delay_stacks_on_original() {
        static ORIGINAL_RECEIVE: AtomicDuration = AtomicDuration::new(Duration::ZERO);
        static DUPLICATE_RECEIVE: AtomicDuration = AtomicDuration::new(Duration::ZERO);
        let startup = Duration::from_nanos(1);

        crate::testing::init_tracing();
        ORIGINAL_RECEIVE.store(Duration::ZERO, Ordering::Relaxed);
        DUPLICATE_RECEIVE.store(Duration::ZERO, Ordering::Relaxed);

        let mut rt = Runtime::new();
        rt.run(|| {
            // Original gets 20ms delay, duplicate gets 5ms delay from monitors.
            // With Relative offset, duplicate delay stacks on top of original's.
            // Duplicate arrives at: base + 20ms (inherited) + 5ms = 75ms
            // Original arrives at: base + 20ms = 70ms
            monitor::on_packet(|packet, operation| {
                if operation == monitor::Operation::Send {
                    if packet.is_duplicate {
                        return monitor::delay(5.ms()).into();
                    }
                    return monitor::delay(20.ms()).into();
                }
                Default::default()
            });
            monitor::on_packet(|packet, operation| {
                if operation == monitor::Operation::Send && !packet.is_duplicate {
                    return monitor::duplicate(1).into();
                }
                Default::default()
            });

            async {
                let socket = UdpSocket::bind("server:8080").await.unwrap();
                let mut data = [0; 4];

                let (len, _) = socket.recv_from(&mut data).await.unwrap();
                assert_eq!(&data[..len], b"ping");
                let first = bach::time::Instant::now().elapsed_since_start();

                let (len, _) = socket.recv_from(&mut data).await.unwrap();
                assert_eq!(&data[..len], b"ping");
                let second = bach::time::Instant::now().elapsed_since_start();

                // Original (20ms + 50ms base) arrives before duplicate (20ms + 5ms + 50ms base)
                ORIGINAL_RECEIVE.store(first, Ordering::Relaxed);
                DUPLICATE_RECEIVE.store(second, Ordering::Relaxed);
            }
            .group("server")
            .primary()
            .spawn();

            async move {
                bach::time::sleep(startup).await;
                let socket = UdpSocket::bind("client:0").await.unwrap();
                socket.send_to(b"ping", "server:8080").await.unwrap();
            }
            .group("client")
            .primary()
            .spawn();
        });

        // base latency = 50ms
        // Original: send_time + 20ms + 50ms base = startup + 70ms
        // Duplicate: send_time + 20ms (relative) + 5ms + 50ms base = startup + 75ms
        assert_eq!(ORIGINAL_RECEIVE.load(Ordering::Relaxed), startup + 70.ms());
        assert_eq!(DUPLICATE_RECEIVE.load(Ordering::Relaxed), startup + 75.ms());
    }

    #[test]
    fn max_duplicates_caps_expansion() {
        static RECEIVED: AtomicUsize = AtomicUsize::new(0);
        let startup = Duration::from_nanos(1);
        // 1 original + DEFAULT_MAX_DUPLICATES (16)
        let expected = 1 + bach::environment::net::monitor::DEFAULT_MAX_DUPLICATES;

        crate::testing::init_tracing();
        RECEIVED.store(0, Ordering::Relaxed);

        let mut rt = Runtime::new();
        rt.run(|| {
            // Request 100 duplicates — should be capped at the default max (16)
            monitor::on_packet_sent(|packet| {
                if !packet.is_duplicate {
                    return monitor::duplicate(100).into();
                }
                Default::default()
            });

            async move {
                let socket = UdpSocket::bind("server:8080").await.unwrap();
                let mut data = [0; 4];
                for _ in 0..expected {
                    let (len, _) = socket.recv_from(&mut data).await.unwrap();
                    assert_eq!(&data[..len], b"ping");
                    RECEIVED.fetch_add(1, Ordering::Relaxed);
                }
                // Verify no additional packets arrive
                assert!(bach::time::timeout(1.s(), socket.recv_from(&mut data))
                    .await
                    .is_err());
            }
            .group("server")
            .primary()
            .spawn();

            async move {
                bach::time::sleep(startup).await;
                let socket = UdpSocket::bind("client:0").await.unwrap();
                socket.send_to(b"ping", "server:8080").await.unwrap();
            }
            .group("client")
            .primary()
            .spawn();
        });

        assert_eq!(RECEIVED.load(Ordering::Relaxed), expected);
    }
}
