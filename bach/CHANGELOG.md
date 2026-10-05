# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
(for `0.x` releases, a breaking change bumps the minor version).

## [0.2.1](https://github.com/camshaft/bach/compare/bach-v0.2.0...bach-v0.2.1) - 2026-10-05

### Other

- move determinism guide into docs/, correct seed-scope mechanics ([#135](https://github.com/camshaft/bach/pull/135))
- add canonical Determinism and Replay guide ([#133](https://github.com/camshaft/bach/pull/133))

## [0.2.0](https://github.com/camshaft/bach/compare/bach-v0.1.3...bach-v0.2.0)

This is a **breaking** release. It supersedes `0.1.3`, which was published as a patch
but in fact carried the breaking changes below and has been **yanked** from crates.io.
Bumping to `0.2.0` restores semver honesty: consumers on a `0.1` caret range no longer
receive the breaking changes under a patch bump.

### ⚠ Breaking changes

- **Remove the `s2n-quic-core` dependency** (#117). The public
  `environment::net::ip::transport::Transport::protocol()` accessor now returns a `u8`
  instead of the removed `s2n_quic_core::inet::Protocol` type. Callers that matched on
  the old enum must update to the numeric protocol value.
- **Remove `Send` constraints from the task/future APIs** (#118) for the single-threaded
  runtime (`F: Future + Send` → `F: Future`, `FnOnce() + Send` → `FnOnce()`, and the
  internal `Send` supertraits). This relaxes the spawn/scheduling bounds for the
  single-threaded discrete-event runtime.

### Features

- Add Callgrind instrumentation with internal-task exclusion (#122).
- Add synchronous group IP lookup for packet monitors (#121).
- Implement UDP GRO (Generic Receive Offload) support (#120).
- Add delayed and duplicated network-monitor commands (#116).

### Fixes

- Scope the `getrandom` wasm backend dependency to the `wasm32` target (#112).

### Internal

- Add the canonical tokenless auto-release pipeline (release-plz + crates.io OIDC
  Trusted Publishing) and restore a green CI baseline (#126, #127, #128, #129).
