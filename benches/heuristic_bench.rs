//! Benchmarks that isolate the `looks_incompressible` entropy heuristic.
//!
//! The heuristic runs on every `&[u8]` / `Vec<u8>` / `String` encode with
//! len >= MIN_COMPRESS_LEN (64). It samples 32 bytes and counts distinct
//! values. These benchmarks measure the cost difference between:
//!   - Payloads below MIN_COMPRESS_LEN (heuristic skipped entirely)
//!   - Random payloads ≥64 bytes (heuristic runs, detects incompressible, skips zstd)
//!   - Compressible payloads ≥64 bytes (heuristic runs, allows zstd)
//!
//! By comparing the "just below threshold" vs "just above threshold" cases
//! for random data, we can isolate the heuristic overhead.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use lencode::io::VecWriter;
use lencode::prelude::*;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use std::hint::black_box;

fn bench_heuristic_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("heuristic");
    let mut rng = StdRng::seed_from_u64(0xE0E0E0E0);

    // Random payloads at various sizes around the MIN_COMPRESS_LEN boundary.
    // The heuristic kicks in at len >= 64. By comparing 63 vs 64 vs 128 vs 256
    // for random data, we can see the entropy-check overhead.
    for &len in &[32usize, 63, 64, 65, 128, 256, 512, 1024] {
        let data: Vec<u8> = (0..len).map(|_| rng.random()).collect();

        group.bench_with_input(BenchmarkId::new("random_encode", len), &data, |b, data| {
            b.iter(|| {
                let mut writer = VecWriter::with_capacity(len + 16);
                data.encode_ext(&mut writer, None).unwrap();
                black_box(writer);
            })
        });
    }

    // Compressible payloads: all zeros. These pass the heuristic (not
    // incompressible) and then go through zstd. By comparing compressible
    // vs random at the same size, we can separate heuristic cost from zstd cost.
    for &len in &[64usize, 128, 256, 512, 1024] {
        let data: Vec<u8> = vec![0u8; len];

        group.bench_with_input(BenchmarkId::new("zeros_encode", len), &data, |b, data| {
            b.iter(|| {
                let mut writer = VecWriter::with_capacity(len + 16);
                data.encode_ext(&mut writer, None).unwrap();
                black_box(writer);
            })
        });
    }

    // Decode side: random raw payloads (no compression). Measures the
    // flagged-header decode overhead at various sizes.
    for &len in &[32usize, 64, 128, 256, 512, 1024] {
        let data: Vec<u8> = (0..len).map(|_| rng.random()).collect();
        let encoded = {
            let mut w = VecWriter::with_capacity(len + 16);
            data.encode_ext(&mut w, None).unwrap();
            w.into_inner()
        };

        group.bench_with_input(
            BenchmarkId::new("random_decode", len),
            &encoded,
            |b, encoded| {
                b.iter(|| {
                    let mut cursor = lencode::io::Cursor::new(black_box(encoded.as_slice()));
                    let v: Vec<u8> = Decode::decode_ext(&mut cursor, None).unwrap();
                    black_box(v);
                })
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_heuristic_overhead);
criterion_main!(benches);
