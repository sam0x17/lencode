#![cfg(feature = "std")]

use borsh::{BorshDeserialize, BorshSerialize};
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use lencode::varint::lencode::Lencode;
use lencode::{Decode, Encode};
use rand::{Rng, rng};
use std::io::Cursor;

fn benchmark_roundup(c: &mut Criterion) {
    let mut group = c.benchmark_group("encoding_vec");

    // Generate random u128 values from random u32 values
    let mut rng = rng();
    let values: Vec<u128> = (0..10000)
        .map(|_| rng.random_range(0..u32::MAX) as u128)
        .collect();

    // Benchmark Borsh encoding
    group.bench_with_input(BenchmarkId::new("borsh", "vec"), &values, |b, values| {
        let mut cursor = Cursor::new(vec![0u8; values.len() * 32]);
        b.iter(|| {
            black_box(values.serialize(&mut cursor).unwrap());
        });
    });

    // Benchmark Bincode encoding
    group.bench_with_input(BenchmarkId::new("bincode", "vec"), &values, |b, values| {
        let mut cursor = Cursor::new(vec![0u8; values.len() * 32]);
        b.iter(|| {
            black_box(
                bincode::encode_into_std_write(values, &mut cursor, bincode::config::standard())
                    .unwrap(),
            );
        });
    });

    // Benchmark Lencode encoding
    group.bench_with_input(BenchmarkId::new("lencode", "vec"), &values, |b, values| {
        let mut cursor = Cursor::new(vec![0u8; values.len() * 32]);
        b.iter(|| {
            black_box(values.encode::<Lencode>(&mut cursor).unwrap());
        });
    });

    group.finish();

    let mut group = c.benchmark_group("decoding_vec");

    // Benchmark Borsh decoding
    group.bench_with_input(BenchmarkId::new("borsh", "vec"), &values, |b, values| {
        b.iter_batched(
            || {
                let mut cursor = Cursor::new(Vec::new());
                values.serialize(&mut cursor).unwrap();
                cursor.into_inner()
            },
            |buf| {
                let mut cursor = Cursor::new(buf);
                black_box(Vec::<u128>::deserialize_reader(&mut cursor).unwrap());
            },
            criterion::BatchSize::SmallInput,
        );
    });

    // Benchmark Bincode decoding
    group.bench_with_input(BenchmarkId::new("bincode", "vec"), &values, |b, values| {
        b.iter_batched(
            || {
                let mut cursor = Cursor::new(Vec::new());
                bincode::encode_into_std_write(values, &mut cursor, bincode::config::standard())
                    .unwrap();
                cursor.into_inner()
            },
            |buf| {
                let mut cursor = Cursor::new(buf);
                black_box(
                    bincode::decode_from_std_read::<Vec<u128>, _, _>(
                        &mut cursor,
                        bincode::config::standard(),
                    )
                    .unwrap(),
                );
            },
            criterion::BatchSize::SmallInput,
        );
    });

    // Benchmark Lencode decoding
    group.bench_with_input(BenchmarkId::new("lencode", "vec"), &values, |b, values| {
        b.iter_batched(
            || {
                let mut cursor = Cursor::new(Vec::new());
                values.encode::<Lencode>(&mut cursor).unwrap();
                cursor.into_inner()
            },
            |buf| {
                let mut cursor = Cursor::new(buf);
                black_box(Vec::<u128>::decode::<Lencode>(&mut cursor).unwrap());
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(benches, benchmark_roundup);

criterion_main!(benches);
