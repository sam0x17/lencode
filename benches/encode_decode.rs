use criterion::{Criterion, black_box, criterion_group, criterion_main};
use lencode::io::Cursor;
use lencode::varint::{Scheme, lencode::Lencode};

fn bench_encode(c: &mut Criterion) {
    c.bench_function("lencode_encode_u64", |b| {
        b.iter_batched(
            || {
                let buf = [0u8; 16];
                let value: u64 = rand::random();
                (buf, value)
            },
            |(mut buf, value)| {
                let mut cursor = Cursor::new(&mut buf[..]);
                black_box(Lencode::encode_varint(value, &mut cursor).unwrap());
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

fn bench_decode(c: &mut Criterion) {
    c.bench_function("lencode_decode_u64", |b| {
        b.iter_batched(
            || {
                let mut buf = [0u8; 16];
                let value: u64 = rand::random();
                let n = {
                    let mut cursor = Cursor::new(&mut buf[..]);
                    Lencode::encode_varint(value, &mut cursor).unwrap()
                };
                (buf, n)
            },
            |(buf, n)| {
                let mut cursor = Cursor::new(&buf[..n]);
                black_box(Lencode::decode_varint::<u64>(&mut cursor).unwrap());
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, bench_encode, bench_decode);
criterion_main!(benches);
