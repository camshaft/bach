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
    use bach::net::monitor;
    use std::{
        io,
        sync::atomic::{AtomicUsize, Ordering},
    };

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
}
