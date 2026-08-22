//! Isolated benchmarks for the deduplication subsystem.
//!
//! These measure `DedupeEncoder::encode` and `DedupeDecoder::decode` overhead
//! separately from the underlying Pack encoding, so we can attribute time to
//! the HashMap lookup / TypeId dispatch / SmallBox downcast chain.

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use lencode::dedupe::{DedupeDecodeable, DedupeEncodeable, DedupeEncoder, DefaultDedupeHasher};
use lencode::io::{Cursor, VecWriter};
use lencode::pack::Pack;
use lencode::prelude::*;
use std::hint::black_box;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Pack)]
#[repr(transparent)]
struct Key32([u8; 32]);

impl DedupeEncodeable for Key32 {
    type Hasher = DefaultDedupeHasher;
}
impl DedupeDecodeable for Key32 {
    type Hasher = DefaultDedupeHasher;
}

fn make_keys(count: usize, seed: u64) -> Vec<Key32> {
    use rand::rngs::StdRng;
    use rand::{RngExt, SeedableRng};
    let mut rng = StdRng::seed_from_u64(seed);
    (0..count).map(|_| Key32(rng.random())).collect()
}

fn make_hotset_keys(count: usize, hotset: &[Key32], hotset_pct: u8, seed: u64) -> Vec<Key32> {
    use rand::rngs::StdRng;
    use rand::seq::SliceRandom;
    use rand::{RngExt, SeedableRng};
    let mut rng = StdRng::seed_from_u64(seed);
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if rng.random_range(0u8..100) < hotset_pct {
            let idx = rng.random_range(0..hotset.len());
            out.push(hotset[idx].clone());
        } else {
            out.push(Key32(rng.random()));
        }
    }
    out.shuffle(&mut rng);
    out
}

// ---------------------------------------------------------------------------
// Encoder benchmarks
// ---------------------------------------------------------------------------

fn bench_dedupe_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("dedupe_encode");

    // Single new-value encode: measures the cold path (hash + insert + pack).
    group.bench_function("single_new_value", |b| {
        let key = Key32([0xAA; 32]);
        b.iter_batched(
            || {
                (
                    DedupeEncoder::with_capacity(64, 1),
                    VecWriter::with_capacity(64),
                )
            },
            |(mut enc, mut writer)| {
                let n = enc
                    .encode::<Key32, DefaultDedupeHasher>(black_box(&key), &mut writer)
                    .unwrap();
                black_box(n);
            },
            BatchSize::SmallInput,
        )
    });

    // Single repeat encode: measures the hot path (hash + lookup + varint ID).
    group.bench_function("single_repeat", |b| {
        let key = Key32([0xBB; 32]);
        b.iter_batched(
            || {
                let mut enc = DedupeEncoder::with_capacity(64, 1);
                let mut scratch = VecWriter::with_capacity(64);
                enc.encode::<Key32, DefaultDedupeHasher>(&key, &mut scratch)
                    .unwrap();
                (enc, VecWriter::with_capacity(64))
            },
            |(mut enc, mut writer)| {
                let n = enc
                    .encode::<Key32, DefaultDedupeHasher>(black_box(&key), &mut writer)
                    .unwrap();
                black_box(n);
            },
            BatchSize::SmallInput,
        )
    });

    // Batch encode 128 keys with 30% hotset (matches solana_bench's scenario).
    let hotset = make_keys(8, 0xD00D);
    let keys = make_hotset_keys(128, &hotset, 30, 0xBEEF);

    group.bench_function("batch_128_hotset30pct", |b| {
        b.iter_batched(
            || {
                (
                    DedupeEncoder::with_capacity(128, 1),
                    VecWriter::with_capacity(128 * 34),
                )
            },
            |(mut enc, mut writer)| {
                for key in &keys {
                    enc.encode::<Key32, DefaultDedupeHasher>(black_box(key), &mut writer)
                        .unwrap();
                }
                black_box(writer);
            },
            BatchSize::SmallInput,
        )
    });

    // Same batch but 0% hotset (all unique — worst case for dedupe).
    let unique_keys = make_keys(128, 0xCAFE);

    group.bench_function("batch_128_all_unique", |b| {
        b.iter_batched(
            || {
                (
                    DedupeEncoder::with_capacity(128, 1),
                    VecWriter::with_capacity(128 * 34),
                )
            },
            |(mut enc, mut writer)| {
                for key in &unique_keys {
                    enc.encode::<Key32, DefaultDedupeHasher>(black_box(key), &mut writer)
                        .unwrap();
                }
                black_box(writer);
            },
            BatchSize::SmallInput,
        )
    });

    // Same batch but 100% hotset (best case for dedupe — almost all hits).
    let repeat_keys = make_hotset_keys(128, &hotset, 100, 0xFACE);

    group.bench_function("batch_128_all_repeats", |b| {
        b.iter_batched(
            || {
                // Pre-seed the encoder with the hotset so every key is a hit.
                let mut enc = DedupeEncoder::with_capacity(128, 1);
                let mut scratch = VecWriter::with_capacity(64);
                for key in &hotset {
                    enc.encode::<Key32, DefaultDedupeHasher>(key, &mut scratch)
                        .unwrap();
                }
                (enc, VecWriter::with_capacity(128 * 4))
            },
            |(mut enc, mut writer)| {
                for key in &repeat_keys {
                    enc.encode::<Key32, DefaultDedupeHasher>(black_box(key), &mut writer)
                        .unwrap();
                }
                black_box(writer);
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Decoder benchmarks
// ---------------------------------------------------------------------------

fn bench_dedupe_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("dedupe_decode");

    // Encode a batch, then decode the entire encoded payload.
    let hotset = make_keys(8, 0xD00D);
    let keys = make_hotset_keys(128, &hotset, 30, 0xBEEF);

    let encoded_hotset30 = {
        let mut enc = DedupeEncoder::with_capacity(128, 1);
        let mut w = VecWriter::with_capacity(128 * 34);
        for key in &keys {
            enc.encode::<Key32, DefaultDedupeHasher>(key, &mut w)
                .unwrap();
        }
        w.into_inner()
    };

    group.bench_function("batch_128_hotset30pct", |b| {
        b.iter_batched(
            || {
                (
                    lencode::dedupe::DedupeDecoder::with_capacity(128),
                    Cursor::new(encoded_hotset30.as_slice()),
                )
            },
            |(mut dec, mut cursor)| {
                for _ in 0..128 {
                    let _key: Key32 = dec.decode(&mut cursor).unwrap();
                }
                black_box(dec);
            },
            BatchSize::SmallInput,
        )
    });

    // All-unique payload (every decode is a cache miss → unpack + store).
    let unique_keys = make_keys(128, 0xCAFE);
    let encoded_unique = {
        let mut enc = DedupeEncoder::with_capacity(128, 1);
        let mut w = VecWriter::with_capacity(128 * 34);
        for key in &unique_keys {
            enc.encode::<Key32, DefaultDedupeHasher>(key, &mut w)
                .unwrap();
        }
        w.into_inner()
    };

    group.bench_function("batch_128_all_unique", |b| {
        b.iter_batched(
            || {
                (
                    lencode::dedupe::DedupeDecoder::with_capacity(128),
                    Cursor::new(encoded_unique.as_slice()),
                )
            },
            |(mut dec, mut cursor)| {
                for _ in 0..128 {
                    let _key: Key32 = dec.decode(&mut cursor).unwrap();
                }
                black_box(dec);
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Encode WITHOUT dedupe (baseline: just Pack::pack for comparison)
// ---------------------------------------------------------------------------

fn bench_pack_baseline(c: &mut Criterion) {
    let mut group = c.benchmark_group("dedupe_vs_pack");

    let hotset = make_keys(8, 0xD00D);
    let keys = make_hotset_keys(128, &hotset, 30, 0xBEEF);

    group.bench_function("pack_only_128", |b| {
        b.iter_batched(
            || VecWriter::with_capacity(128 * 34),
            |mut writer| {
                for key in &keys {
                    black_box(key).pack(&mut writer).unwrap();
                }
                black_box(writer);
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("dedupe_128_hotset30pct", |b| {
        b.iter_batched(
            || {
                (
                    DedupeEncoder::with_capacity(128, 1),
                    VecWriter::with_capacity(128 * 34),
                )
            },
            |(mut enc, mut writer)| {
                for key in &keys {
                    enc.encode::<Key32, DefaultDedupeHasher>(black_box(key), &mut writer)
                        .unwrap();
                }
                black_box(writer);
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_dedupe_encode,
    bench_dedupe_decode,
    bench_pack_baseline
);
criterion_main!(benches);
