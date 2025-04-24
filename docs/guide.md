## 1. Introduction to Discrete Event Simulations

### What is a Discrete Event Simulation (DES)?
A **Discrete Event Simulation (DES)** models a system by simulating discrete events that occur at specific points in time, rather than continuous changes. Events are state changes (e.g., a packet being sent, a note being played) scheduled and executed in a controlled, non-real-time environment. Bach uses DES to test async systems, such as network protocols or music sequencers, with deterministic outcomes.

### Why is it helpful?
- **Determinism**: DES ensures repeatable results by controlling event timing, unlike real-time systems with unpredictable delays.
- **Testing Complex Systems**: Allows simulation of edge cases (e.g., packet loss, task conflicts) without real-world hardware.
- **Flexibility**: Models diverse domains, from music composition to network protocols, in a single framework.
- **Debugging**: Simulated time and event logs simplify tracing system behavior.

### How to Use It in Bach
Bach’s DES is driven by the `bach::sim` function, which runs a closure containing async tasks representing events. Tasks are scheduled using simulated time, and the simulation terminates when all primary tasks complete, dropping secondary tasks.

**Example: Basic Task Simulation**

```rust
use bach::ext::*;
use std::time::Duration;

#[test]
fn basic_des() {
    bach::sim(|| {
        async {
            println!("Event at 1s");
            sleep(Duration::from_secs(1)).await;
            println!("Event at 2s");
            sleep(Duration::from_secs(1)).await;
        }
        .primary()
        .spawn();

        async {
            println!("Secondary event at 5s (will be dropped)");
            sleep(Duration::from_secs(5)).await;
        }
        .spawn();
    });
}
```
- **What it does**: Simulates two tasks. The primary task prints messages at 1s and 2s (simulated time). The secondary task is dropped at 2s when the primary task completes.
- **Why it’s helpful**: Demonstrates DES’s ability to control event timing and prioritize critical tasks, useful for testing music sequences or network protocols.

---

## 2. How Time Works in a Discrete Event Simulator

### What is Time in a DES?
In a DES, time is **simulated** rather than real. Bach advances time to the next scheduled event, executing it instantly in real time, regardless of the simulated duration. This is managed by an event scheduler that maintains a queue of events with timestamps, ensuring deterministic execution.

### Why is it Helpful?
- **Speed**: Long durations (e.g., hours) are simulated in milliseconds, enabling fast testing.
- **Precision**: Exact control over event timing avoids real-world jitter.
- **Repeatability**: Simulated time ensures consistent results across runs.
- **Debugging**: Timestamps in logs or PCAPs help trace event sequences.

### How to Use It in Bach
Bach’s `sleep` function (e.g., `bach::time::sleep`) schedules events at specific simulated times. The scheduler advances time to the next event, executing tasks in order of their timestamps and priority (primary tasks first).

**Example: Timed Network Events**
```rust
use bach::{ext::*, net::UdpSocket};
use std::time::Duration;

#[test]
fn timed_ping() {
    bach::sim(|| {
        async {
            let socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();
            sleep(Duration::from_millis(100)).await; // Delay send
            socket.send_to(b"ping", "server:8080").await.unwrap();
        }
        .primary()
        .spawn();

        async {
            let socket = UdpSocket::bind("server:8080").await.unwrap();
            let mut data = [0; 4];
            let (len, addr) = socket.recv_from(&mut data).await.unwrap();
            println!("Received at {}ms", bach::time::elapsed().as_millis());
            assert_eq!(&data[..len], b"ping");
        }
        .primary()
        .spawn();
    });
}
```
- **What it does**: A client sends a "ping" after a 100ms delay, and the server receives it. The server logs the simulated time (`elapsed`), which is ~100ms.
- **Why it’s helpful**: Shows precise control over event timing, critical for testing network protocols (e.g., QUIC timeouts in `aws/s2n-quic`) or music event scheduling in `euphony-rs`.

---

## 3. Queue Facilities in Bach

### What are Bach’s Queue Facilities?
Bach provides a composable queue system via `Queue::builder()`, allowing users to construct queues with customizable behaviors like capacity, latency, packet loss, mutex protection, and sojourn time tracking. Queues are used for task communication, network simulation, and system modeling.

### Why is it Helpful?
- **Flexibility**: Layer features (e.g., latency, loss) without modifying code, enabling diverse use cases like network channels or music event queues.
- **Realism**: Simulates real-world constraints (e.g., network delays, queue overflows).
- **Metrics**: Sojourn tracking provides performance insights, useful for visualization in `camshaft/kew`.
- **Concurrency**: Mutex support ensures thread-safe access in concurrent tests.

### How to Use It in Bach
The `Queue::builder()` API supports methods like `.with_capacity()`, `.latent()`, `.with_packet_loss()`, `.mutex()`, `.sojourn()`, and `.channel()` to create sender/receiver pairs.

**Example: Network Queue with Latency and Loss**
```rust
use bach::{ext::*, queue::vec_deque::Queue};
use std::time::Duration;

#[test]
fn network_queue() {
    bach::sim(|| {
        let (mut sender, mut receiver) = Queue::builder()
            .with_capacity(Some(10))
            .build()
            .latent(Duration::from_millis(10))
            .with_packet_loss(0.1)
            .mutex()
            .sojourn()
            .channel();

        async move {
            for i in 0..5 {
                sender.send(i).await.unwrap();
                sleep(Duration::from_millis(5)).await;
            }
        }
        .primary()
        .spawn_named("sender");

        async move {
            let mut received = 0;
            while let Ok(data) = receiver.pop().await {
                received += 1;
                println!("Received {} after {}ms", data, bach::time::elapsed().as_millis());
            }
            assert!(received <= 5, "Account for packet loss");
        }
        .primary()
        .spawn_named("receiver");
    });
}
```
- **What it does**: Simulates a sender transmitting 5 integers through a queue with 10ms latency and 10% packet loss. The receiver counts received items, expecting up to 5 due to potential losses.
- **Why it’s helpful**: Models network communication (like `aws/s2n-quic` packet queues) or music event pipelines (like `euphony-rs` note sequencing), with realistic constraints and metrics.

---

## 4. Randomness Provided by Bolero

### What is Bolero’s Randomness in Bach?
Bach integrates with [Bolero](https://github.com/BurntSushi/bolero), a Rust testing framework that provides randomness for both exhaustive and non-exhaustive testing. Bolero supports:
- **Exhaustive Testing**: Tests all possible task interleavings (as seen in `interleavings` tests).
- **Non-Exhaustive Fuzzing**: Uses engines like [libFuzzer](https://llvm.org/docs/LibFuzzer.html) to generate random inputs or schedules.
- **Corpus Replay**: Replays saved input sets for regression testing.
- **RNG Input Generation**: Provides basic random inputs during `cargo test` runs.

### Why is it Helpful?
- **Comprehensive Testing**: Exhaustive testing catches all concurrency bugs, while fuzzing explores edge cases efficiently.
- **Reproducibility**: Corpus replay ensures consistent debugging of failures.
- **Flexibility**: RNG inputs enable quick tests without external fuzzing tools.
- **Scalability**: Non-exhaustive fuzzing handles large systems where exhaustive testing is infeasible (e.g., `aws/s2n-quic`).

### How to Use It in Bach
Bolero’s `check!()` macro configures testing modes:
- `.exhaustive()`: Tests all interleavings.
- `.with_fuzzer()`: Enables libFuzzer for random inputs.
- `.replay(corpus)`: Replays saved inputs.
- Default `cargo test` uses RNG inputs.

**Example: Fuzzing Network Interleavings**
```rust
use bach::{ext::*, net::UdpSocket};
use bolero::check;

#[test]
fn fuzz_ping_pong() {
    check!()
        .with_fuzzer() // Use libFuzzer for random schedules
        .run(bach::sim(|| {
            async {
                let socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();
                socket.send_to(b"ping", "server:8080").await.unwrap();
                let mut data = [0; 4];
                let (len, _) = socket.recv_from(&mut data).await.unwrap();
                assert_eq!(&data[..len], b"pong");
            }
            .primary()
            .spawn();

            async {
                let socket = UdpSocket::bind("server:8080").await.unwrap();
                let mut data = [0; 4];
                let (len, addr) = socket.recv_from(&mut data).await.unwrap();
                socket.send_to(b"pong", addr).await.unwrap();
            }
            .primary()
            .spawn();
        }));
}
```
- **What it does**: Uses libFuzzer to randomly schedule client and server tasks, testing UDP ping-pong under various interleavings. Failures are saved for corpus replay.
- **Why it’s helpful**: Quickly identifies concurrency bugs in network protocols (like `aws/s2n-quic`) without exhaustive testing, with replay for debugging.

---

## 5. Partial Order Reduction

### What is Partial Order Reduction (POR)?
[Partial Order Reduction](https://en.wikipedia.org/wiki/Partial_order_reduction) reduces the number of task interleavings tested in concurrent systems by only exploring schedules where tasks conflict on shared resources (e.g., a queue). Bach implements POR using a [disjoint set data structure](https://en.wikipedia.org/wiki/Disjoint-set_data_structure), grouping tasks and resources that access the same object.

### Why is it Helpful?
- **Efficiency**: Drastically reduces the combinatorial explosion of interleavings, making exhaustive testing feasible.
- **Correctness**: Preserves all distinct system behaviors (e.g., deadlocks, race conditions).
- **Scalability**: Handles complex systems with many tasks, like `aws/s2n-quic` or `camshaft/kew`.
- **Simplicity**: Disjoint sets are more efficient than dependency graphs for dynamic grouping.

### How to Use It in Bach
Bach’s `Runtime` with cooperative scheduling (`with_coop(true)`) enables POR. When a task accesses a resource (e.g., queue push/pop), it joins the resource’s set via `union`. Only tasks in the same set are interleaved.

**Example: Queue Interleaving with POR**
```rust
use bach::{ext::*, queue::vec_deque::Queue};
use bolero::check;

#[test]
fn queue_por() {
    check!().exhaustive().run(bach::sim(|| {
        let (mut sender1, mut receiver) = Queue::builder().build().mutex().channel();
        let (mut sender2, _) = Queue::builder().build().mutex().channel(); // Separate queue

        async move {
            sender1.send(1).await.unwrap();
        }
        .primary()
        .spawn_named("sender1");

        async move {
            sender2.send(2).await.unwrap();
        }
        .primary()
        .spawn_named("sender2");

        async move {
            if let Ok(data) = receiver.pop().await {
                println!("Received {}", data);
            }
        }
        .primary()
        .spawn_named("receiver");
    }));
}
```
- **What it does**: Tests interleavings of two senders and a receiver. Sender1 and receiver share a queue (same set), so their operations are interleaved. Sender2 uses a separate queue (different set), so its interleavings are skipped.
- **Why it’s helpful**: Reduces test time by focusing on queue conflicts, applicable to music event scheduling (`euphony-rs`) or network packet queues (`aws/s2n-quic`).

---

## 6. Network Simulation

### What is Bach’s Network Simulation?
Bach’s network simulation models UDP communication using `UdpSocket`, built on its composable queue system. It supports configurable latency, packet loss, reordering, and duplication, with PCAP exporting (`BACH_PCAP_DIR`) for packet analysis. TCP support is planned for the near future.

### Why is it Helpful?
- **Realism**: Simulates network conditions (e.g., 10% packet loss) for protocol testing.
- **Debugging**: PCAPs enable Wireshark analysis of packet flows.
- **Flexibility**: Queue-based design allows easy extension (e.g., future TCP support).
- **Integration**: Supports `aws/s2n-quic` testing and `camshaft/kew` visualization.

### How to Use It in Bach
Use `UdpSocket` for network tasks, configure queues with `.with_packet_loss()` or `.latent()`, and enable PCAP exporting. Monitors (`monitor::on_packet_sent`) track or inject faults.

**Example: Network with Packet Loss and PCAP**
```rust
use bach::{ext::*, net::{monitor, UdpSocket}};
use std::time::Duration;

#[test]
fn lossy_network() {
    std::env::set_var("BACH_PCAP_DIR", "./pcaps");
    static SENT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    bach::sim(|| {
        monitor::on_packet_sent(|_| {
            SENT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Default::default()
        });

        async {
            let socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();
            // Hypothetical API for packet loss
            socket.set_queue_options(bach::queue::QueueOptions::new().with_packet_loss(0.1));
            for _ in 0..10 {
                socket.send_to(b"data", "server:8080").await.unwrap();
                sleep(Duration::from_millis(1)).await;
            }
        }
        .primary()
        .spawn();

        async {
            let socket = UdpSocket::bind("server:8080").await.unwrap();
            let mut received = 0;
            for _ in 0..10 {
                let mut data = [0; 4];
                if socket.recv_from(&mut data).await.is_ok() {
                    received += 1;
                }
            }
            assert!(received <= 10, "Account for packet loss");
            assert_eq!(SENT.load(std::sync::atomic::Ordering::Relaxed), 10);
        }
        .primary()
        .spawn();
    });
}
```
- **What it does**: Simulates a client sending 10 UDP packets with 10% packet loss, tracked by a monitor. The server counts received packets, expecting up to 10. A PCAP file captures traffic.
- **Why it’s helpful**: Tests network protocol resilience (like `aws/s2n-quic`), with PCAPs aiding debugging and monitors providing metrics.

---

## Conclusion
Bach’s components—discrete event simulation, simulated time, composable queues, Bolero randomness, POR, and network simulation—form a powerful framework for testing async systems. Each component addresses specific needs:
- **DES and Time**: Ensure deterministic, fast simulations.
- **Queues**: Model communication with realism and metrics.
- **Bolero**: Provides flexible testing modes.
- **POR**: Optimizes concurrency testing.
- **Network**: Simulates realistic UDP communication, with TCP planned.

These features support diverse applications, from music composition in `camshaft/euphony-rs` to protocol testing in `aws/s2n-quic` and visualization in `camshaft/kew`. To get started, explore the [Bach repository](https://github.com/camshaft/bach) and try the examples above.
