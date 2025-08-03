use criterion::{Criterion, black_box, criterion_group, criterion_main};
use lencode::io::Cursor;
use lencode::varint::{Scheme, lencode::Lencode};

fn bench_encode(c: &mut Criterion) {
    let mut buf = [0u8; 16];
    c.bench_function("lencode_encode_u64", |b| {
        b.iter(|| {
            let mut cursor = Cursor::new(&mut buf[..]);
            let value: u64 = black_box(0xFEDC_BA98_7654_3210);
            black_box(Lencode::encode_varint(value, &mut cursor).unwrap());
        })
    });
}

fn bench_decode(c: &mut Criterion) {
    let mut buf = [0u8; 16];
    let value: u64 = 0xFEDC_BA98_7654_3210;
    let n = {
        let mut cursor = Cursor::new(&mut buf[..]);
        Lencode::encode_varint(value, &mut cursor).unwrap()
    };
    c.bench_function("lencode_decode_u64", |b| {
        b.iter(|| {
            let mut cursor = Cursor::new(&buf[..n]);
            black_box(Lencode::decode_varint::<u64>(&mut cursor).unwrap());
        })
    });
}

criterion_group!(benches, bench_encode, bench_decode);
criterion_main!(benches);
