# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.3](https://github.com/camshaft/bach/compare/bach-v0.1.2...bach-v0.1.3) - 2026-09-28

### Added

- add Callgrind instrumentation with internal task exclusion ([#122](https://github.com/camshaft/bach/pull/122))
- add synchronous group IP lookup for packet monitors ([#121](https://github.com/camshaft/bach/pull/121))
- Implement UDP GRO (Generic Receive Offload) support ([#120](https://github.com/camshaft/bach/pull/120))
- [**breaking**] Remove `Send` constraints from task/future APIs for single-threaded runtime ([#118](https://github.com/camshaft/bach/pull/118))
- add delayed and duplicated network monitor commands ([#116](https://github.com/camshaft/bach/pull/116))

### Other

- set bach version to 0.1.3 (release version) ([#131](https://github.com/camshaft/bach/pull/131))
- bump bach version to 0.2.0 ([#125](https://github.com/camshaft/bach/pull/125))
- remove s2n-quic-core dependency ([#117](https://github.com/camshaft/bach/pull/117))
