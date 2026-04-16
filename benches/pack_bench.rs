//! Benchmarks for the Pack trait and its bulk operations.
//!
//! Tests `pack`, `unpack`, `pack_slice`, and `unpack_vec` for primitive arrays
//! and `#[repr(transparent)]` newtypes, isolating the Pack layer from the
//! Encode/Decode/Dedupe layers above it.

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use lencode::dedupe::{DefaultDedupeHasher, DedupeDecodeable, DedupeEncodeable};
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

#[derive(Clone, Debug, PartialEq, Eq, Hash, Pack)]
#[repr(transparent)]
struct Key64([u8; 64]);

impl DedupeEncodeable for Key64 {
    type Hasher = DefaultDedupeHasher;
}
impl DedupeDecodeable for Key64 {
    type Hasher = DefaultDedupeHasher;
}

fn make_key32s(count: usize) -> Vec<Key32> {
    use rand::rngs::StdRng;
    use rand::{RngExt, SeedableRng};
    let mut rng = StdRng::seed_from_u64(0xBAC4);
    (0..count).map(|_| Key32(rng.random())).collect()
}

fn make_key64s(count: usize) -> Vec<Key64> {
    use rand::rngs::StdRng;
    use rand::{RngExt, SeedableRng};
    let mut rng = StdRng::seed_from_u64(0xBAC464);
    (0..count).map(|_| Key64(rng.random())).collect()
}

// ---------------------------------------------------------------------------
// Single-value pack / unpack
// ---------------------------------------------------------------------------

fn bench_pack_single(c: &mut Criterion) {
    let mut group = c.benchmark_group("pack_single");

    // [u8; 32] pack via buf_mut fast path
    let key = Key32([0xCC; 32]);
    group.bench_function("key32_pack", |b| {
        b.iter_batched(
            || Cursor::new([0u8; 32]),
            |cursor| {
                let mut cursor = black_box(cursor);
                black_box(key.pack(&mut cursor).unwrap());
            },
            BatchSize::SmallInput,
        )
    });

    // [u8; 32] unpack via buf fast path
    let mut enc_buf = [0u8; 32];
    key.pack(&mut Cursor::new(&mut enc_buf[..])).unwrap();
    group.bench_function("key32_unpack", |b| {
        b.iter_batched(
            || Cursor::new(black_box(enc_buf)),
            |cursor| {
                let mut cursor = black_box(cursor);
                black_box(Key32::unpack(&mut cursor).unwrap());
            },
            BatchSize::SmallInput,
        )
    });

    // [u8; 64] pack
    let key64 = Key64([0xDD; 64]);
    group.bench_function("key64_pack", |b| {
        b.iter_batched(
            || Cursor::new([0u8; 64]),
            |cursor| {
                let mut cursor = black_box(cursor);
                black_box(key64.pack(&mut cursor).unwrap());
            },
            BatchSize::SmallInput,
        )
    });

    // Primitive u64 pack (uses endian_cast, NOT varint)
    let val: u64 = 0x123456789ABCDEF0;
    group.bench_function("u64_pack", |b| {
        b.iter_batched(
            || Cursor::new([0u8; 8]),
            |cursor| {
                let mut cursor = black_box(cursor);
                black_box(val.pack(&mut cursor).unwrap());
            },
            BatchSize::SmallInput,
        )
    });

    // Primitive u64 unpack
    let mut u64_buf = [0u8; 8];
    val.pack(&mut Cursor::new(&mut u64_buf[..])).unwrap();
    group.bench_function("u64_unpack", |b| {
        b.iter_batched(
            || Cursor::new(black_box(u64_buf)),
            |cursor| {
                let mut cursor = black_box(cursor);
                black_box(u64::unpack(&mut cursor).unwrap());
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Bulk pack_slice / unpack_vec
// ---------------------------------------------------------------------------

fn bench_pack_bulk(c: &mut Criterion) {
    let mut group = c.benchmark_group("pack_bulk");

    for &count in &[16usize, 64, 128, 512] {
        let keys = make_key32s(count);
        let total_bytes = count * 32;

        // pack_slice: bulk write of count * 32 bytes
        group.bench_with_input(
            BenchmarkId::new("key32_pack_slice", count),
            &keys,
            |b, keys| {
                b.iter_batched(
                    || VecWriter::with_capacity(total_bytes),
                    |mut writer| {
                        let n = Key32::pack_slice(black_box(keys), &mut writer).unwrap();
                        black_box(n);
                    },
                    BatchSize::SmallInput,
                )
            },
        );

        // unpack_vec: bulk read of count * 32 bytes
        let packed = {
            let mut w = VecWriter::with_capacity(total_bytes);
            Key32::pack_slice(&keys, &mut w).unwrap();
            w.into_inner()
        };

        group.bench_with_input(
            BenchmarkId::new("key32_unpack_vec", count),
            &packed,
            |b, packed| {
                b.iter_batched(
                    || Cursor::new(packed.as_slice()),
                    |cursor| {
                        let mut cursor = black_box(cursor);
                        let v: Vec<Key32> = Key32::unpack_vec(&mut cursor, count).unwrap();
                        black_box(v);
                    },
                    BatchSize::SmallInput,
                )
            },
        );
    }

    // Compare per-item pack loop vs bulk pack_slice for 128 keys
    let keys128 = make_key32s(128);
    group.bench_function("key32_pack_loop_128", |b| {
        b.iter_batched(
            || VecWriter::with_capacity(128 * 32),
            |mut writer| {
                for key in black_box(&keys128) {
                    key.pack(&mut writer).unwrap();
                }
                black_box(writer);
            },
            BatchSize::SmallInput,
        )
    });

    // [u8; 64] bulk pack_slice for 128 keys
    let keys64 = make_key64s(128);
    group.bench_function("key64_pack_slice_128", |b| {
        b.iter_batched(
            || VecWriter::with_capacity(128 * 64),
            |mut writer| {
                let n = Key64::pack_slice(black_box(&keys64), &mut writer).unwrap();
                black_box(n);
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Encode (varint length prefix + pack_slice) vs raw pack_slice
// ---------------------------------------------------------------------------

fn bench_encode_vs_pack(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_vs_pack");

    let keys = make_key32s(128);

    // Via Encode trait (adds varint length header, routes through encode_slice → pack_slice)
    group.bench_function("vec_key32_encode_128", |b| {
        b.iter_batched(
            || VecWriter::with_capacity(128 * 32 + 16),
            |mut writer| {
                keys.encode_ext(&mut writer, None).unwrap();
                black_box(writer);
            },
            BatchSize::SmallInput,
        )
    });

    // Via raw pack_slice (no length header, just the bytes)
    group.bench_function("vec_key32_pack_slice_128", |b| {
        b.iter_batched(
            || VecWriter::with_capacity(128 * 32),
            |mut writer| {
                Key32::pack_slice(black_box(&keys), &mut writer).unwrap();
                black_box(writer);
            },
            BatchSize::SmallInput,
        )
    });

    // Decode comparison
    let encoded = {
        let mut w = VecWriter::with_capacity(128 * 32 + 16);
        keys.encode_ext(&mut w, None).unwrap();
        w.into_inner()
    };

    group.bench_function("vec_key32_decode_128", |b| {
        b.iter_batched(
            || Cursor::new(encoded.as_slice()),
            |cursor| {
                let mut cursor = black_box(cursor);
                let v: Vec<Key32> = Decode::decode_ext(&mut cursor, None).unwrap();
                black_box(v);
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(benches, bench_pack_single, bench_pack_bulk, bench_encode_vs_pack);
criterion_main!(benches);
