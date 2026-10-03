# Determinism and Replay

Bach is a discrete-event simulation runtime. Its central guarantee is that a
simulation is **reproducible**: the same seed, run against the same source,
produces the same execution every time, so a failure recorded as a seed can be
replayed and turned into a regression test.

This document is the canonical description of what Bach makes deterministic,
what it cannot, and how to diagnose a run that fails to reproduce. Consumers
that drive their tests through Bach (for example, running [bolero][bolero] seeds
against a Bach simulation) should treat the "Leaks" section below as the
checklist to run before calling a non-reproducing seed "flaky" — in a Bach
simulation it almost never is.

[bolero]: https://github.com/camshaft/bolero

## What Bach controls

Determinism rests on three sources. The scheduler and clock are deterministic
unconditionally; the third, the RNG, becomes deterministic once you install a
seed:

1. **Scheduler / executor** (`bach/src/executor.rs`, `bach/src/task/supervisor.rs`).
   Tasks are polled in a deterministic order driven by the simulated run and
   event queues (`VecDeque`, `pop_front`), never by OS threads. There is no real
   parallelism, so there is no thread-scheduling nondeterminism. This holds
   whether or not a seed is set.

2. **Simulated clock** (`bach/src/time.rs`). Time advances in discrete
   simulated steps. Any sleep, timeout, or `Instant`-equivalent resolves
   through Bach's clock rather than wall-clock time, so timing is identical on
   every run regardless of how fast or loaded the host is. This also holds
   whether or not a seed is set.

3. **Seeded RNG** (`bach/src/rand.rs`). bach draws its randomness from a seeded
   `Xoshiro256PlusPlus` (`seed_from_u64`) wrapped as a bolero generator driver,
   from which `bolero_generator` draws (`produce()`, `any()`, and the rest of
   the re-exported `bach::rand` prelude) pull. There are two ways to install the
   seed, at different granularity:
   - **Runtime-wide** (`Runtime::with_seed`, `bach/src/environment/default.rs`):
     a single scope stored on the environment and entered once around each
     macrostep's poll loop, so every task draws from one shared, deterministic
     stream in poll order.
   - **Per-future** (`future.with_seed(seed)` via `SeedExt`, `bach/src/ext.rs`):
     wraps one future in its own scope that is entered on every poll of that
     future, giving it an independent deterministic stream.
   Either way the stream is a pure function of the seed.

With a seed installed, a Bach run is a pure function of `(seed, source)`; the
scheduler and clock are already deterministic without one.

## Seeding and replaying

RNG determinism is not automatic. `bach::sim(..)` builds the default runtime,
which is **unseeded** (`rand: None`): no RNG scope is installed, so `any()` /
`produce()` draws are not pinned by any Bach seed. The deterministic scheduler
and simulated clock still apply, but to make the random draws reproducible you
must install a seed, either runtime-wide:

```rust
let mut rt = bach::environment::default::Runtime::new().with_seed(seed);
rt.run(|| { /* simulation */ });
```

or per-future with `future.with_seed(seed)`.

Under bolero, the harness supplies the seed. Per bolero's own semantics (see the
[bolero][bolero] crate — these are bolero's behavior, not defined in bach), a
recorded `BOLERO_RANDOM_SEED` regenerates the exact failing input as bolero's
first input, and `BOLERO_RANDOM_ITERATIONS` counts the distinct random seeds
tried rather than inputs per seed — so the seed itself, not `ITERATIONS=1`, is
what pins the replay. Setting the recorded seed on matching source is sufficient
to replay.

The replay contract is: **same seed + same source**, where "source" means the
same code, the same bolero / generator-crate versions, *and the same build
configuration* — the `cfg` flags and features the seed was recorded under. Build
configuration matters because `cfg`/feature gates can change which code paths
execute, and therefore which draws are taken from the seeded stream and in what
order; a change that alters the executed paths yields a different generated
input, so the seed stops reproducing. (A `cfg` difference that touches no
executed path is harmless.) That is a source mismatch, not a determinism bug.

One easy-to-miss pitfall when reproducing inside a managed build system: Cargo
does not merge `rustflags` — it uses a single highest-priority source
(`CARGO_ENCODED_RUSTFLAGS`, then `RUSTFLAGS`, then `target.*.rustflags`, then
`[build] rustflags`) and ignores the rest. So a bare `RUSTFLAGS` env var wholly
*replaces* a build wrapper's configured `[build] rustflags` rather than adding to
them, silently dropping flags the build needs or changing the `cfg` the seed was
recorded under — the test then fails to compile or runs under a different
configuration. Because the sources do not combine automatically, build the full
flag list yourself and pass it as one source: for example, under Amazon's brazil,
set `CARGO_ENCODED_RUSTFLAGS` to brazil's configured flags plus your added
`--cfg`, rather than a bare `RUSTFLAGS` that clobbers them.

## Leaks: what Bach cannot control

Determinism holds only for entropy and ordering that flow through the three
controlled sources above. Bach cannot intercept nondeterminism in the
system-under-test that bypasses them. Bach's own source references no
`thread_rng`, `getrandom`, or `SystemTime`, and no wall-clock `Instant::now` — it
supplies the deterministic primitives — so in practice a leak almost always
lives in the code being simulated rather than in Bach. (Bach does use a few
`std` `HashMap`/`HashSet` instances internally, for example in `group.rs`, but
never in a way that exposes hash iteration order to a simulation decision — the
only such iteration, `group::list()`, feeds diagnostic snapshots.) In priority
order:

1. **`std` HashMap / HashSet iteration order — the number-one culprit.**
   `std::collections::HashMap` and `HashSet` default to `RandomState`, which is
   seeded once per process from OS `getrandom` and is invisible to the Bach
   seed. Any time iteration order over such a collection drives a decision — the
   order tasks are spawned in, the order operations are applied or committed, a
   choice of element — the run diverges across processes at a fixed seed. Fix:
   use `BTreeMap` / `BTreeSet`, or `IndexMap`, or a fixed-seed `BuildHasher`,
   anywhere the order is observable.

   Note a subtle variant: routing a *shuffle* through the seeded RNG is
   necessary but not sufficient. A deterministic shuffle only yields a
   deterministic result if its **input order** is already deterministic.
   Shuffling a `Vec` that was built by iterating a HashMap is still
   nondeterministic. Verify that the collection feeding any shuffle was built in
   a deterministic order (from a `Vec`/`BTree`, or explicitly sorted first).

2. **Wall-clock time.** `SystemTime::now()`, `Instant::now()`, or `std::time`
   used directly instead of Bach's `time` module. Any timestamp or timeout
   derived from real time is nondeterministic.

3. **OS threads and real concurrency.** `std::thread::spawn`, `tokio`, blocking
   I/O, real sockets — anything the OS schedules rather than Bach's executor. A
   stray `tokio` runtime or spawned thread inside a Bach test is a common
   smoking gun.

4. **Direct RNG not drawn from the Bach scope.** `rand::thread_rng()`,
   `getrandom`, or a default-seeded `fastrand`.

5. **Pointer / address-dependent behavior.** Hashing by pointer, a map keyed on
   an address, `Debug`-formatting an address, or any ordering that depends on
   allocation addresses (ASLR). Rare, but real.

A workspace-wide clippy `disallowed-types` lint banning `std` `HashMap` /
`HashSet` / `RandomState` (with an explicit, justified `allow` for the genuinely
order-insensitive cases) is an effective preventive net against class 1.

## Diagnosing a seed that will not reproduce

If a recorded seed does not reproduce even on matching source, do **not** label
the test flaky. Use this check, which needs no exact source revision:

Because `RandomState` is seeded once **per process**, run the same seed in many
**fresh processes** on the same source — for example, a shell loop invoking the
test binary ~20 times with `BOLERO_RANDOM_SEED` set — and compare outcomes.
Bach guarantees that same seed + same source + same process is identical, so:

- If the pass/fail result, or the sequence of applied operations, **flips across
  fresh processes**, a per-process entropy leak is confirmed (a `RandomState`
  HashMap-order leak is the first suspect). That is a sim-determinism bug to
  root-cause and fix, per the leak taxonomy above — not flakiness.
- If it is **stable across processes** but still differs from the recorded
  failure, the recorded input came from a different source or
  bolero/generator-crate version. That is a source mismatch; resolve the source
  revision rather than hunting a leak.

This cleanly tells you which case you are in before you spend effort on either.

## Turning a recorded seed into a regression test

You do not need the original source revision to lock in a fix. Once the fix is
in place, write a named deterministic test that reproduces the mechanism (either
the recorded seed replayed on the fixed source, or a hand-built minimal case
that exercises the same path) and check it in. Reserve the exact-revision replay
for confirming the *original* input failed before the fix; proving pass-after is
done on the fixed source.
