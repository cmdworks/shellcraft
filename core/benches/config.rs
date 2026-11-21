use criterion::{criterion_group, criterion_main, Criterion};
use shellcraft_core::cache::{decode, encode, SourceId};
use shellcraft_core::{Config, DEFAULT_CONFIG};
use std::hint::black_box;

fn bench(c: &mut Criterion) {
    let src = SourceId {
        len: DEFAULT_CONFIG.len() as u64,
        mtime_ns: 1,
    };
    let cfg = Config::from_toml(DEFAULT_CONFIG).expect("default config");
    let bytes = encode(&cfg, src).expect("encode");

    c.bench_function("parse_toml_default", |b| {
        b.iter(|| Config::from_toml(black_box(DEFAULT_CONFIG)).expect("parse"));
    });
    c.bench_function("decode_cache_default", |b| {
        b.iter(|| decode(black_box(&bytes), src).expect("decode"));
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
