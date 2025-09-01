use criterion::{Criterion, black_box, criterion_group, criterion_main};
use lencode::{
    dedupe::{DedupeDecoder, DedupeEncoder},
    prelude::*,
};
use solana_sdk::pubkey::Pubkey;

fn bench_encode_pubkey(c: &mut Criterion) {
    c.bench_function("lencode_encode_pubkey", |b| {
        b.iter_batched(
            || {
                let cursor = Cursor::new([0u8; 64]);
                let value: Pubkey = Pubkey::new_unique();
                let dedupe_encoder = DedupeEncoder::new();
                (cursor, value, dedupe_encoder)
            },
            |(mut cursor, value, mut dedupe_encoder)| {
                black_box(
                    value
                        .encode(&mut cursor, Some(&mut dedupe_encoder))
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
                    let mut dedupe_encoder = DedupeEncoder::new();
                    value
                        .encode(&mut cursor, Some(&mut dedupe_encoder))
                        .unwrap();
                }
                let cursor = Cursor::new(buf);
                let dedupe_decoder = DedupeDecoder::new();
                (cursor, dedupe_decoder)
            },
            |(mut cursor, mut dedupe_decoder)| {
                black_box(Pubkey::decode(&mut cursor, Some(&mut dedupe_decoder)).unwrap());
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, bench_encode_pubkey, bench_decode_pubkey);
criterion_main!(benches);
