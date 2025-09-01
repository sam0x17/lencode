use criterion::{Criterion, black_box, criterion_group, criterion_main};
use lencode::prelude::*;
use solana_sdk::pubkey::Pubkey;

fn bench_encode_pubkey(c: &mut Criterion) {
    c.bench_function("lencode_encode_pubkey", |b| {
        b.iter_batched(
            || {
                let cursor = Cursor::new([0u8; 64]);
                let value: Pubkey = Pubkey::new_unique();
                (cursor, value)
            },
            |(mut cursor, value)| {
                black_box(value.encode(&mut cursor, None).unwrap());
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
                    value.encode(&mut cursor, None).unwrap();
                }
                Cursor::new(buf)
            },
            |mut cursor| {
                black_box(Pubkey::decode(&mut cursor, None).unwrap());
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, bench_encode_pubkey, bench_decode_pubkey);
criterion_main!(benches);
