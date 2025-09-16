#![cfg(feature = "std")]

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use lencode::prelude::*;
use rand::{Rng, rng};
use std::collections::VecDeque;
use std::hint::black_box;

fn bench_bytes_encoding(c: &mut Criterion) {
    let mut group = c.benchmark_group("bytes_encode");

    // Small random (likely raw)
    let rand_small: Vec<u8> = (0..256).map(|_| rand::rng().random()).collect();
    group.bench_with_input(
        BenchmarkId::new("slice", "rand_small"),
        &rand_small,
        |b, data| {
            b.iter(|| {
                let mut buf = Vec::new();
                black_box((&data[..]).encode(&mut buf).unwrap());
                black_box(buf)
            })
        },
    );

    // Large zeros (compressible)
    let zeros: Vec<u8> = vec![0; 64 * 1024];
    group.bench_with_input(BenchmarkId::new("slice", "zeros_64k"), &zeros, |b, data| {
        b.iter(|| {
            let mut buf = Vec::new();
            black_box((&data[..]).encode(&mut buf).unwrap());
            black_box(buf)
        })
    });

    // Vec<u8> random
    let mut rng = rng();
    let rand_big: Vec<u8> = (0..32 * 1024).map(|_| rng.random()).collect();
    group.bench_with_input(BenchmarkId::new("vec", "rand_32k"), &rand_big, |b, data| {
        b.iter(|| {
            let mut buf = Vec::new();
            black_box(data.encode(&mut buf).unwrap());
            black_box(buf)
        })
    });

    // Vec<u8> repeated pattern
    let patt: Vec<u8> = (0..128)
        .flat_map(|i| core::iter::repeat(i as u8).take(256))
        .collect();
    group.bench_with_input(BenchmarkId::new("vec", "pattern"), &patt, |b, data| {
        b.iter(|| {
            let mut buf = Vec::new();
            black_box(data.encode(&mut buf).unwrap());
            black_box(buf)
        })
    });

    // VecDeque<u8> zeros
    let deq_zeros: VecDeque<u8> = std::iter::repeat(0u8).take(128 * 1024).collect();
    group.bench_with_input(
        BenchmarkId::new("vecdeque", "zeros_128k"),
        &deq_zeros,
        |b, data| {
            b.iter(|| {
                let mut buf = Vec::new();
                black_box(data.encode(&mut buf).unwrap());
                black_box(buf)
            })
        },
    );

    group.finish();
}

fn bench_bytes_decoding(c: &mut Criterion) {
    let mut group = c.benchmark_group("bytes_decode");

    // Prepare encoded buffers
    let rand_small: Vec<u8> = (0..256).map(|_| rand::rng().random()).collect();
    let zeros: Vec<u8> = vec![0; 64 * 1024];
    let rand_big: Vec<u8> = (0..32 * 1024).map(|_| rand::rng().random()).collect();
    let patt: Vec<u8> = (0..128)
        .flat_map(|i| core::iter::repeat(i as u8).take(256))
        .collect();
    let deq_zeros: VecDeque<u8> = std::iter::repeat(0u8).take(128 * 1024).collect();

    let enc_rand_small = {
        let mut buf = Vec::new();
        (&rand_small[..]).encode(&mut buf).unwrap();
        buf
    };
    let enc_zeros = {
        let mut buf = Vec::new();
        (&zeros[..]).encode(&mut buf).unwrap();
        buf
    };
    let enc_rand_big = {
        let mut buf = Vec::new();
        rand_big.encode(&mut buf).unwrap();
        buf
    };
    let enc_patt = {
        let mut buf = Vec::new();
        patt.encode(&mut buf).unwrap();
        buf
    };
    let enc_deq_zeros = {
        let mut buf = Vec::new();
        deq_zeros.encode(&mut buf).unwrap();
        buf
    };

    group.bench_function("slice_rand_small", |b| {
        b.iter(|| black_box(Vec::<u8>::decode(&mut Cursor::new(&enc_rand_small))).unwrap())
    });
    group.bench_function("slice_zeros_64k", |b| {
        b.iter(|| black_box(Vec::<u8>::decode(&mut Cursor::new(&enc_zeros))).unwrap())
    });
    group.bench_function("vec_rand_32k", |b| {
        b.iter(|| black_box(Vec::<u8>::decode(&mut Cursor::new(&enc_rand_big))).unwrap())
    });
    group.bench_function("vec_pattern", |b| {
        b.iter(|| black_box(Vec::<u8>::decode(&mut Cursor::new(&enc_patt))).unwrap())
    });
    group.bench_function("vecdeque_zeros_128k", |b| {
        b.iter(|| black_box(VecDeque::<u8>::decode(&mut Cursor::new(&enc_deq_zeros))).unwrap())
    });

    group.finish();
}

criterion_group!(benches, bench_bytes_encoding, bench_bytes_decoding);
criterion_main!(benches);
