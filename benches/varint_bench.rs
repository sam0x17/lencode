use criterion::{Criterion, criterion_group, criterion_main};
use lencode::prelude::*;
use rand::{Rng, rng};
use std::hint::black_box;

fn random_u64_split(rng: &mut impl Rng) -> u64 {
    let half = u64::MAX / 2;
    if rng.random() {
        rng.random_range(0..=half)
    } else {
        rng.random_range((half + 1)..=u64::MAX)
    }
}

fn bench_encode(c: &mut Criterion) {
    let mut rng = rng();
    c.bench_function("lencode_encode_u64", |b| {
        b.iter_batched(
            || {
                let cursor = Cursor::new([0u8; 16]);
                let value = random_u64_split(&mut rng);
                (cursor, value)
            },
            |(mut cursor, value)| {
                black_box(Lencode::encode_varint(value, &mut cursor).unwrap());
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

fn bench_decode(c: &mut Criterion) {
    let mut rng = rng();
    c.bench_function("lencode_decode_u64", |b| {
        b.iter_batched(
            || {
                let mut buf = [0u8; 16];
                let value = random_u64_split(&mut rng);
                {
                    let mut cursor = Cursor::new(&mut buf[..]);
                    Lencode::encode_varint(value, &mut cursor).unwrap();
                }
                Cursor::new(buf)
            },
            |mut cursor| {
                black_box(Lencode::decode_varint::<u64>(&mut cursor).unwrap());
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, bench_encode, bench_decode);
criterion_main!(benches);
