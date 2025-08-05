use borsh::BorshSerialize;
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use lencode::io::Cursor;
use lencode::io::Write;
use rand::{Rng, rng};

fn benchmark_encoding(c: &mut Criterion) {
    let mut group = c.benchmark_group("encoding");

    // Generate random u128 values from random u32 values
    let mut rng = rng();
    let values: Vec<u128> = (0..100)
        .map(|_| rng.random_range(0..u32::MAX) as u128)
        .collect();

    // Benchmark Borsh encoding
    group.bench_with_input(BenchmarkId::new("borsh", "vec"), &values, |b, values| {
        b.iter(|| {
            let mut buf = Vec::new();
            black_box(values.serialize(&mut buf).unwrap());
        });
    });

    // Benchmark Lencode encoding
    group.bench_with_input(BenchmarkId::new("lencode", "vec"), &values, |b, values| {
        b.iter(|| {
            let mut buf = vec![0u8; 16 * values.len()];
            let mut cursor = Cursor::new(&mut buf);
            for val in values {
                let bytes = val.to_le_bytes();
                black_box(cursor.write(&bytes).unwrap());
            }
        });
    });

    group.finish();
}

criterion_group!(benches, benchmark_encoding);

criterion_main!(benches);
