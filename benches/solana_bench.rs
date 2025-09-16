use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use lencode::{
    dedupe::{DedupeDecoder, DedupeEncoder},
    prelude::*,
};
use rand::Rng;
use rand::seq::SliceRandom;
use solana_sdk::pubkey::Pubkey;
use std::hint::black_box;
use std::io::Cursor;

use borsh::BorshDeserialize;

fn bench_encode_pubkey(c: &mut Criterion) {
    c.bench_function("lencode_encode_pubkey", |b| {
        b.iter_batched(
            || {
                let cursor = Cursor::new([0u8; 64]);
                let value: Pubkey = Pubkey::new_unique();
                let dedupe_encoder = DedupeEncoder::with_capacity(10, 1);
                (cursor, value, dedupe_encoder)
            },
            |(mut cursor, value, mut dedupe_encoder)| {
                black_box(
                    value
                        .encode_ext(&mut cursor, Some(&mut dedupe_encoder))
                        .unwrap(),
                );
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

fn bench_decode_pubkey(c: &mut Criterion) {
    c.bench_function("lencode_decode_pubkey", |b| {
        b.iter_batched(
            || {
                let mut buf = [0u8; 64];
                let value: Pubkey = Pubkey::new_unique();
                {
                    let mut cursor = Cursor::new(&mut buf[..]);
                    let mut dedupe_encoder = DedupeEncoder::with_capacity(10, 1);
                    value
                        .encode_ext(&mut cursor, Some(&mut dedupe_encoder))
                        .unwrap();
                }
                let cursor = Cursor::new(buf);
                let dedupe_decoder = DedupeDecoder::with_capacity(10);
                (cursor, dedupe_decoder)
            },
            |(mut cursor, mut dedupe_decoder)| {
                black_box(Pubkey::decode_ext(&mut cursor, Some(&mut dedupe_decoder)).unwrap());
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

fn benchmark_pubkey_vec_with_duplicates(c: &mut Criterion) {
    // Create a vector of 1000 pubkeys where 50% are duplicates
    // This simulates real-world scenarios where many transactions
    // reference the same popular accounts/programs
    let mut rng = rand::rng();
    let unique_pubkeys: Vec<Pubkey> = (0..500).map(|_| Pubkey::new_unique()).collect();

    // Create duplicates by randomly selecting from unique pubkeys
    let mut duplicates: Vec<Pubkey> = (0..500)
        .map(|_| {
            let idx = rng.random_range(0..unique_pubkeys.len());
            unique_pubkeys[idx]
        })
        .collect();

    // Combine and shuffle to create realistic distribution
    let mut all_pubkeys = unique_pubkeys;
    all_pubkeys.append(&mut duplicates);
    all_pubkeys.shuffle(&mut rng);

    let mut group = c.benchmark_group("encoding_pubkey_vec_50pct_duplicates");

    // Benchmark borsh encoding
    group.bench_with_input(
        BenchmarkId::new("borsh", "pubkey_vec"),
        &all_pubkeys,
        |b, pubkeys| b.iter(|| black_box(borsh::to_vec(pubkeys).unwrap())),
    );

    // Benchmark lencode encoding with deduplication
    group.bench_with_input(
        BenchmarkId::new("lencode", "pubkey_vec"),
        &all_pubkeys,
        |b, pubkeys| {
            b.iter_batched(
                || {
                    let encoder = DedupeEncoder::with_capacity(1000, 1);
                    let cursor = Cursor::new(Vec::new());
                    (encoder, cursor)
                },
                |(mut encoder, mut cursor)| {
                    black_box(pubkeys.encode_ext(&mut cursor, Some(&mut encoder)).unwrap());
                    cursor.into_inner()
                },
                criterion::BatchSize::SmallInput,
            )
        },
    );

    group.finish();

    // Benchmark decoding
    let mut group = c.benchmark_group("decoding_pubkey_vec_50pct_duplicates");

    // Prepare encoded data for both formats
    let borsh_data = borsh::to_vec(&all_pubkeys).unwrap();

    let lencode_data = {
        let mut encoder = DedupeEncoder::with_capacity(1000, 1);
        let mut cursor = Cursor::new(Vec::new());
        all_pubkeys
            .encode_ext(&mut cursor, Some(&mut encoder))
            .unwrap();
        cursor.into_inner()
    };

    // Benchmark borsh decoding
    group.bench_with_input(
        BenchmarkId::new("borsh", "pubkey_vec"),
        &borsh_data,
        |b, data| b.iter(|| black_box(Vec::<Pubkey>::try_from_slice(data).unwrap())),
    );

    // Benchmark lencode decoding with deduplication
    group.bench_with_input(
        BenchmarkId::new("lencode", "pubkey_vec"),
        &lencode_data,
        |b, data| {
            b.iter_batched(
                || {
                    let decoder = DedupeDecoder::with_capacity(1000);
                    let cursor = Cursor::new(data);
                    (decoder, cursor)
                },
                |(mut decoder, mut cursor)| {
                    black_box(Vec::<Pubkey>::decode_ext(&mut cursor, Some(&mut decoder)).unwrap())
                },
                criterion::BatchSize::SmallInput,
            )
        },
    );

    group.finish();
}

fn benchmark_compiled_instruction_data(c: &mut Criterion) {
    // Prepare two datasets: compressible and random data inside CompiledInstruction
    let mut rng = rand::rng();
    let compressible_data: Vec<u8> = vec![0; 64 * 1024];
    let random_data: Vec<u8> = (0..64 * 1024).map(|_| rng.random()).collect();

    let mk_ixs = |data: &Vec<u8>| -> Vec<solana_sdk::instruction::CompiledInstruction> {
        (0..64)
            .map(|i| solana_sdk::instruction::CompiledInstruction {
                program_id_index: (i % 5) as u8,
                accounts: vec![0, 1, 2, 3, 4],
                data: data.clone(),
            })
            .collect()
    };

    let ixs_compressible = mk_ixs(&compressible_data);
    let ixs_random = mk_ixs(&random_data);

    let mut group = c.benchmark_group("solana_compiled_instruction_data");

    group.bench_function("lencode_encode_compressible", |b| {
        b.iter(|| {
            let mut cursor = Cursor::new(Vec::new());
            black_box(ixs_compressible.encode(&mut cursor).unwrap());
            cursor.into_inner()
        })
    });

    group.bench_function("lencode_encode_random", |b| {
        b.iter(|| {
            let mut cursor = Cursor::new(Vec::new());
            black_box(ixs_random.encode(&mut cursor).unwrap());
            cursor.into_inner()
        })
    });

    let enc_compressible = {
        let mut cursor = Cursor::new(Vec::new());
        ixs_compressible.encode(&mut cursor).unwrap();
        cursor.into_inner()
    };
    let enc_random = {
        let mut cursor = Cursor::new(Vec::new());
        ixs_random.encode(&mut cursor).unwrap();
        cursor.into_inner()
    };

    group.bench_function("lencode_decode_compressible", |b| {
        b.iter_batched(
            || Cursor::new(enc_compressible.clone()),
            |mut cursor| {
                black_box(Vec::<solana_sdk::instruction::CompiledInstruction>::decode(
                    &mut cursor,
                ))
                .unwrap()
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("lencode_decode_random", |b| {
        b.iter_batched(
            || Cursor::new(enc_random.clone()),
            |mut cursor| {
                black_box(Vec::<solana_sdk::instruction::CompiledInstruction>::decode(
                    &mut cursor,
                ))
                .unwrap()
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_encode_pubkey,
    bench_decode_pubkey,
    benchmark_pubkey_vec_with_duplicates,
    benchmark_compiled_instruction_data
);
criterion_main!(benches);
