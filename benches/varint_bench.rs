use criterion::{Criterion, criterion_group, criterion_main};
use lencode::prelude::*;
use rand::{Rng, rng};
use std::hint::black_box;
#[allow(unused_imports)]
use wincode::SchemaWrite;

macro_rules! bench_type {
    ($c:expr, $rng:expr, $($ty:ty),+) => {
        $(
            {
                let value: $ty = $rng.random();

                let mut group = $c.comparison_benchmark_group(
                    format!("{}_encode", stringify!($ty))
                );
                group.bench_function("lencode", |b| {
                    b.iter_batched(
                        || Cursor::new([0u8; 32]),
                        |mut cursor| {
                            black_box(value.encode(&mut cursor).unwrap());
                        },
                        criterion::BatchSize::SmallInput,
                    )
                });
                group.bench_function("wincode", |b| {
                    b.iter_batched(
                        || [0u8; 32],
                        |mut buf| {
                            let mut writer: &mut [u8] = &mut buf;
                            black_box(wincode::serialize_into(&mut writer, &value).unwrap());
                        },
                        criterion::BatchSize::SmallInput,
                    )
                });
                group.finish();

                let mut enc_buf = [0u8; 32];
                {
                    let mut cursor = Cursor::new(&mut enc_buf[..]);
                    value.encode(&mut cursor).unwrap();
                }
                let mut wincode_buf = [0u8; 32];
                {
                    let mut writer: &mut [u8] = &mut wincode_buf;
                    wincode::serialize_into(&mut writer, &value).unwrap();
                }

                let mut group = $c.comparison_benchmark_group(
                    format!("{}_decode", stringify!($ty))
                );
                group.bench_function("lencode", |b| {
                    b.iter_batched(
                        || Cursor::new(enc_buf),
                        |mut cursor| {
                            black_box(<$ty>::decode(&mut cursor).unwrap());
                        },
                        criterion::BatchSize::SmallInput,
                    )
                });
                group.bench_function("wincode", |b| {
                    b.iter_batched(
                        || wincode_buf,
                        |buf| {
                            let mut reader: &[u8] = &buf;
                            black_box(wincode::deserialize_from::<$ty>(&mut reader).unwrap());
                        },
                        criterion::BatchSize::SmallInput,
                    )
                });
                group.finish();
            }
        )+
    };
}

fn bench_unsigned(c: &mut Criterion) {
    let mut rng = rng();
    bench_type!(c, rng, u8, u16, u32, u64, u128);
}

fn bench_signed(c: &mut Criterion) {
    let mut rng = rng();
    bench_type!(c, rng, i8, i16, i32, i64, i128);
}

fn bench_bool(c: &mut Criterion) {
    let mut rng = rng();
    let value: bool = rng.random();

    let mut group = c.comparison_benchmark_group("bool_encode");
    group.bench_function("lencode", |b| {
        b.iter_batched(
            || Cursor::new([0u8; 1]),
            |mut cursor| {
                black_box(value.encode(&mut cursor).unwrap());
            },
            criterion::BatchSize::SmallInput,
        )
    });
    group.bench_function("wincode", |b| {
        b.iter_batched(
            || [0u8; 1],
            |mut buf| {
                let mut writer: &mut [u8] = &mut buf;
                black_box(wincode::serialize_into(&mut writer, &value).unwrap());
            },
            criterion::BatchSize::SmallInput,
        )
    });
    group.finish();

    let mut enc_buf = [0u8; 1];
    {
        let mut cursor = Cursor::new(&mut enc_buf[..]);
        value.encode(&mut cursor).unwrap();
    }
    let mut wincode_buf = [0u8; 1];
    {
        let mut writer: &mut [u8] = &mut wincode_buf;
        wincode::serialize_into(&mut writer, &value).unwrap();
    }

    let mut group = c.comparison_benchmark_group("bool_decode");
    group.bench_function("lencode", |b| {
        b.iter_batched(
            || Cursor::new(enc_buf),
            |mut cursor| {
                black_box(<bool>::decode(&mut cursor).unwrap());
            },
            criterion::BatchSize::SmallInput,
        )
    });
    group.bench_function("wincode", |b| {
        b.iter_batched(
            || wincode_buf,
            |buf| {
                let mut reader: &[u8] = &buf;
                black_box(wincode::deserialize_from::<bool>(&mut reader).unwrap());
            },
            criterion::BatchSize::SmallInput,
        )
    });
    group.finish();
}

fn bench_float(c: &mut Criterion) {
    let mut rng = rng();

    {
        let value: f32 = rng.random::<f32>() * 1e6 - 5e5;

        let mut group = c.comparison_benchmark_group("f32_encode");
        group.bench_function("lencode", |b| {
            b.iter_batched(
                || Cursor::new([0u8; 4]),
                |mut cursor| {
                    black_box(value.encode(&mut cursor).unwrap());
                },
                criterion::BatchSize::SmallInput,
            )
        });
        group.bench_function("wincode", |b| {
            b.iter_batched(
                || [0u8; 4],
                |mut buf| {
                    let mut writer: &mut [u8] = &mut buf;
                    black_box(wincode::serialize_into(&mut writer, &value).unwrap());
                },
                criterion::BatchSize::SmallInput,
            )
        });
        group.finish();

        let mut enc_buf = [0u8; 4];
        {
            let mut cursor = Cursor::new(&mut enc_buf[..]);
            value.encode(&mut cursor).unwrap();
        }
        let mut wincode_buf = [0u8; 4];
        {
            let mut writer: &mut [u8] = &mut wincode_buf;
            wincode::serialize_into(&mut writer, &value).unwrap();
        }

        let mut group = c.comparison_benchmark_group("f32_decode");
        group.bench_function("lencode", |b| {
            b.iter_batched(
                || Cursor::new(enc_buf),
                |mut cursor| {
                    black_box(<f32>::decode(&mut cursor).unwrap());
                },
                criterion::BatchSize::SmallInput,
            )
        });
        group.bench_function("wincode", |b| {
            b.iter_batched(
                || wincode_buf,
                |buf| {
                    let mut reader: &[u8] = &buf;
                    black_box(wincode::deserialize_from::<f32>(&mut reader).unwrap());
                },
                criterion::BatchSize::SmallInput,
            )
        });
        group.finish();
    }

    {
        let value: f64 = rng.random::<f64>() * 1e12 - 5e11;

        let mut group = c.comparison_benchmark_group("f64_encode");
        group.bench_function("lencode", |b| {
            b.iter_batched(
                || Cursor::new([0u8; 8]),
                |mut cursor| {
                    black_box(value.encode(&mut cursor).unwrap());
                },
                criterion::BatchSize::SmallInput,
            )
        });
        group.bench_function("wincode", |b| {
            b.iter_batched(
                || [0u8; 8],
                |mut buf| {
                    let mut writer: &mut [u8] = &mut buf;
                    black_box(wincode::serialize_into(&mut writer, &value).unwrap());
                },
                criterion::BatchSize::SmallInput,
            )
        });
        group.finish();

        let mut enc_buf = [0u8; 8];
        {
            let mut cursor = Cursor::new(&mut enc_buf[..]);
            value.encode(&mut cursor).unwrap();
        }
        let mut wincode_buf = [0u8; 8];
        {
            let mut writer: &mut [u8] = &mut wincode_buf;
            wincode::serialize_into(&mut writer, &value).unwrap();
        }

        let mut group = c.comparison_benchmark_group("f64_decode");
        group.bench_function("lencode", |b| {
            b.iter_batched(
                || Cursor::new(enc_buf),
                |mut cursor| {
                    black_box(<f64>::decode(&mut cursor).unwrap());
                },
                criterion::BatchSize::SmallInput,
            )
        });
        group.bench_function("wincode", |b| {
            b.iter_batched(
                || wincode_buf,
                |buf| {
                    let mut reader: &[u8] = &buf;
                    black_box(wincode::deserialize_from::<f64>(&mut reader).unwrap());
                },
                criterion::BatchSize::SmallInput,
            )
        });
        group.finish();
    }
}

criterion_group!(
    benches,
    bench_unsigned,
    bench_signed,
    bench_bool,
    bench_float
);
criterion_main!(benches);
