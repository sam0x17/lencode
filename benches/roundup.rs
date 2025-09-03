#![cfg(feature = "std")]

use borsh::{BorshDeserialize, BorshSerialize};
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use lencode::{Decode, Encode};
use rand::seq::SliceRandom;
use rand::{Rng, rng};
use std::io::Cursor;

fn benchmark_roundup(c: &mut Criterion) {
    let mut group = c.benchmark_group("encoding_vec");

    // generate a fair dataset of random u128 values
    let mut rng1 = rng();
    let mut rng2 = rng();
    let mut rng3 = rng();
    let mut rng4 = rng();
    let mut values: Vec<u128> = (0..1000)
        .map(|_| rng1.random_range(0..u8::MAX) as u128)
        .chain((0..1000).map(|_| rng2.random_range(0..u32::MAX) as u128))
        .chain((0..1000).map(|_| rng3.random_range(0..u64::MAX) as u128))
        .chain((0..1000).map(|_| rng4.random_range(0..u128::MAX)))
        .chain(0..1000)
        .map(|_| 0 as u128)
        .collect();
    values.shuffle(&mut rand::rng());

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
            black_box(values.encode_ext(&mut cursor, None).unwrap());
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
                values.encode_ext(&mut cursor, None).unwrap();
                cursor.into_inner()
            },
            |buf| {
                let mut cursor = Cursor::new(buf);
                black_box(Vec::<u128>::decode_ext(&mut cursor, None).unwrap());
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(benches, benchmark_roundup);

criterion_main!(benches);
