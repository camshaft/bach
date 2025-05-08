# Bach

**Bach** is a Rust-based framework for simulating and testing complex async/await-based systems in a non-real-time environment. It's capable of modeling network protocols, queueing systems, and concurrent task interactions with testing and visualization tools.

## Key Features
- **Discrete Event Simulation**: Schedules events in simulated time for deterministic testing.
- **Async/Await Integration**: Supports any async that doesn't require a specific runtime, like `tokio` or `async-std`.
- **Composable Queues**: Build flexible queues with latency, packet loss, mutexes, and sojourn tracking.
- **Network Simulation**: Simulates UDP sockets with configurable latency, loss, reordering, and duplication; TCP support planned for the near future.
- **[Partial Order Reduction](https://en.wikipedia.org/wiki/Partial_order_reduction)**: Optimizes task interleaving testing using a [disjoint set](https://en.wikipedia.org/wiki/Disjoint-set_data_structure), reducing the search space for conflicting resource accesses.
- **PCAP Exporting**: Captures simulated network traffic as PCAP files for analysis with Wireshark.
- **[Bolero](https://github.com/camshaft/bolero) Integration**: Enables exhaustive and non-exhaustive (fuzzing-based) interleaving testing using engines like [libFuzzer](https://llvm.org/docs/LibFuzzer.html), with corpus replay and basic RNG input generation for `cargo test`.
- **Monitoring**: Tracks packet sends, socket reads/writes, and supports fault injection.
- **WASM Support**: Can compile to WebAssembly and run in the browser for interactive simulations

## Example: Simulating a Networked Ping-Pong

This example simulates two clients sending "ping" to a server over UDP, which responds with "pong".

```rust
use bach::{ext::*, net::UdpSocket};

#[test]
fn ping_pong() {
    bach::sim(|| {
        for i in 0..2 {
            async move {
                let socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();
                socket.send_to(b"ping", "server:8080").await.unwrap();
                let mut data = [0; 4];
                let (len, _) = socket.recv_from(&mut data).await.unwrap();
                assert_eq!(&data[..len], b"pong");
            }
            .group(format!("client_{i}"))
            .primary()
            .spawn();
        }

        async {
            let socket = UdpSocket::bind("server:8080").await.unwrap();
            loop {
                let mut data = [0; 4];
                let (len, addr) = socket.recv_from(&mut data).await.unwrap();
                assert_eq!(&data[..len], b"ping");
                socket.send_to(b"pong", addr).await.unwrap();
            }
        }
        .group("server")
        .spawn();
    });
}
```

This test can be executed with the following command, while also exporting pcaps showing the interaction between the tasks:

```bash
$ BACH_PCAP_DIR=target/bach/pcaps cargo test
```

## Installation

Add to `Cargo.toml`:
```toml
[dependencies]
bach = "0.1"
```

## Related Projects

- **[aws/s2n-quic](https://github.com/aws/s2n-quic)**: A QUIC protocol implementation. Employs Bach’s UDP-based network simulation to test high-level protocol behaviors, such as correctness, using PCAP exporting, fault injection, and [monte-carlo simulations](https://dnglbrstg7yg.cloudfront.net/8f7696e6d3163286a915ec29a6d0bd709c946ee8/sim/index.html#network_jitter/duration.json).
- **[camshaft/kew](https://github.com/camshaft/kew)**: A book about queueing theory. Utilizes Bach to simulate and visualize queue behaviors (e.g., FIFO, priority queues) in a browser via WebAssembly.
- **[camshaft/euphony-rs](https://github.com/camshaft/euphony-rs)**: A music composition environment. Uses Bach to schedule musical events (e.g., notes, rhythms) in non-real-time simulations for algorithmic composition.

Explore the [Bach repository](https://github.com/camshaft/bach) for more details or to contribute!
