use borsh::BorshSerialize;
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use lencode::Decode;
use lencode::varint::lencode::Lencode;
use lencode::{Encode, io::Cursor};
use rand::{Rng, rng};

fn benchmark_roundup(c: &mut Criterion) {
    let mut group = c.benchmark_group("encoding_vec");

    // Generate random u128 values from random u32 values
    let mut rng = rng();
    let values: Vec<u128> = (0..10)
        .map(|_| rng.random_range(0..u16::MAX) as u128)
        .collect();

    // Benchmark Borsh encoding
    group.bench_with_input(BenchmarkId::new("borsh", "vec"), &values, |b, values| {
        let mut buf = vec![0u8; 1000];
        b.iter(|| {
            black_box(values.serialize(&mut buf).unwrap());
        });
    });

    // Benchmark Bincode encoding
    group.bench_with_input(BenchmarkId::new("bincode", "vec"), &values, |b, values| {
        let mut buf = vec![0u8; 1000];
        b.iter(|| {
            black_box(
                bincode::encode_into_slice(values, &mut buf, bincode::config::standard()).unwrap(),
            );
        });
    });

    // Benchmark Lencode encoding
    group.bench_with_input(BenchmarkId::new("lencode", "vec"), &values, |b, values| {
        let mut buf = vec![0u8; 1000];
        b.iter(|| {
            let mut cursor = Cursor::new(&mut buf);
            black_box(values.encode::<Lencode>(&mut cursor).unwrap());
        });
    });

    group.finish();

    let mut group = c.benchmark_group("decoding_vec");

    // Benchmark Borsh encoding
    group.bench_with_input(BenchmarkId::new("borsh", "vec"), &values, |b, values| {
        b.iter_batched(
            || {
                let mut buf = Vec::new();
                values.serialize(&mut buf).unwrap();
                buf
            },
            |buf| {
                black_box(
                    <Vec<u128> as borsh::BorshDeserialize>::deserialize(&mut buf.as_slice())
                        .unwrap(),
                );
            },
            criterion::BatchSize::SmallInput,
        );
    });

    // Benchmark Bincode decoding
    group.bench_with_input(BenchmarkId::new("bincode", "vec"), &values, |b, values| {
        b.iter_batched(
            || bincode::encode_to_vec(values, bincode::config::standard()).unwrap(),
            |buf| {
                black_box(
                    bincode::decode_from_slice::<Vec<u128>, bincode::config::Configuration>(
                        &buf,
                        bincode::config::standard(),
                    )
                    .unwrap()
                    .0,
                );
            },
            criterion::BatchSize::SmallInput,
        );
    });

    // Benchmark Lencode decoding
    group.bench_with_input(BenchmarkId::new("lencode", "vec"), &values, |b, values| {
        b.iter_batched(
            || {
                let mut buf = vec![0u8; 510];
                let mut cursor = Cursor::new(&mut buf);
                values.encode::<Lencode>(&mut cursor).unwrap();
                buf
            },
            |buf| {
                let mut cursor = Cursor::new(&buf);
                black_box(Vec::<u128>::decode::<Lencode>(&mut cursor).unwrap());
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(benches, benchmark_roundup);

criterion_main!(benches);
