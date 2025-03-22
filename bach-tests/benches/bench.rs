// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use criterion::{criterion_group, criterion_main};

criterion_group!(benches, bach_tests::benches::run);
criterion_main!(benches);
