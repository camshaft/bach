use criterion::{criterion_group, criterion_main};

criterion_group!(benches, bach_tests::benches::run);
criterion_main!(benches);
