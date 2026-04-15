//! Micro-benchmarks for the `lencode::io` traits and adapters.
//!
//! These exercise `Cursor` (read + write) and `VecWriter` in isolation, with
//! `black_box` applied to inputs/outputs so the compiler can't fold them away.
//! The goal is to measure the overhead of the I/O layer itself — `buf()`,
//! `buf_mut()`, `advance`, `advance_mut`, `read`, `write`, and growth — without
//! the encoding logic on top.
//!
//! Builds against both the default (no_std + alloc) and `std` feature sets;
//! the I/O surface this benches is the same in both because `VecWriter` is
//! backed by `alloc::vec::Vec` regardless.

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use lencode::io::{Cursor, Read, VecWriter, Write};
use std::hint::black_box;

// ---------------------------------------------------------------------------
// Cursor read benchmarks
// ---------------------------------------------------------------------------

fn bench_cursor_read(c: &mut Criterion) {
    let mut group = c.benchmark_group("cursor_read");

    // `buf()` in isolation: returns the remaining slice.
    group.bench_function("buf", |b| {
        let backing = [0u8; 256];
        b.iter_batched(
            || Cursor::new(black_box(backing)),
            |cursor| {
                let cursor = black_box(cursor);
                black_box(cursor.buf());
            },
            BatchSize::SmallInput,
        )
    });

    // `advance(n)` in isolation: bumps the position field.
    group.bench_function("advance_8", |b| {
        let backing = [0u8; 256];
        b.iter_batched(
            || Cursor::new(black_box(backing)),
            |cursor| {
                let mut cursor = black_box(cursor);
                cursor.advance(8);
                black_box(cursor);
            },
            BatchSize::SmallInput,
        )
    });

    // `read(&mut [u8])` for various target sizes via the trait method (NOT the
    // zero-copy `buf()` path). Forces the slow path that actually copies bytes.
    for &len in &[1usize, 4, 8, 16, 32, 64, 256] {
        group.bench_with_input(BenchmarkId::new("read_into", len), &len, |b, &len| {
            let backing = [0u8; 256];
            b.iter_batched(
                || (Cursor::new(black_box(backing)), [0u8; 256]),
                |(mut cursor, mut dst)| {
                    let _ = cursor.read(&mut dst[..len]).unwrap();
                    black_box(dst);
                },
                BatchSize::SmallInput,
            )
        });
    }

    // Sequential single-byte reads via `buf()` + `advance(1)`. This is the
    // pattern that varint decode loops would hit if they read byte-by-byte.
    group.bench_function("buf_advance_loop_32", |b| {
        let backing = [0u8; 256];
        b.iter_batched(
            || Cursor::new(black_box(backing)),
            |cursor| {
                let mut cursor = black_box(cursor);
                let mut sum: u32 = 0;
                for _ in 0..32 {
                    let slice = cursor.buf().unwrap();
                    sum = sum.wrapping_add(slice[0] as u32);
                    cursor.advance(1);
                }
                black_box(sum);
            },
            BatchSize::SmallInput,
        )
    });

    // Bulk read pattern: one `buf()` + memcpy + `advance(n)` mimicking the
    // varint fast path.
    group.bench_function("buf_memcpy_advance_8", |b| {
        let backing = [0u8; 256];
        b.iter_batched(
            || (Cursor::new(black_box(backing)), [0u8; 8]),
            |(mut cursor, mut dst)| {
                let cursor_ref = &mut cursor;
                let slice = cursor_ref.buf().unwrap();
                dst.copy_from_slice(&slice[..8]);
                cursor_ref.advance(8);
                black_box(dst);
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Cursor write benchmarks (writes into a fixed-size backing buffer)
// ---------------------------------------------------------------------------

fn bench_cursor_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("cursor_write");

    // `buf_mut()` in isolation: returns the remaining mutable slice.
    group.bench_function("buf_mut", |b| {
        b.iter_batched(
            || Cursor::new(black_box([0u8; 256])),
            |cursor| {
                let mut cursor = black_box(cursor);
                black_box(cursor.buf_mut());
            },
            BatchSize::SmallInput,
        )
    });

    // `advance_mut(n)`: bumps position without writing.
    group.bench_function("advance_mut_8", |b| {
        b.iter_batched(
            || Cursor::new(black_box([0u8; 256])),
            |cursor| {
                let mut cursor = black_box(cursor);
                cursor.advance_mut(8);
                black_box(cursor);
            },
            BatchSize::SmallInput,
        )
    });

    // `write(&[u8])` via the trait method for various sizes.
    for &len in &[1usize, 4, 8, 16, 32, 64, 256] {
        group.bench_with_input(BenchmarkId::new("write", len), &len, |b, &len| {
            let src = [0xAAu8; 256];
            b.iter_batched(
                || Cursor::new([0u8; 256]),
                |cursor| {
                    let mut cursor = black_box(cursor);
                    let _ = cursor.write(black_box(&src[..len])).unwrap();
                    black_box(cursor);
                },
                BatchSize::SmallInput,
            )
        });
    }

    // `buf_mut() + memcpy + advance_mut(n)` — the varint encode fast path.
    group.bench_function("buf_mut_memcpy_advance_8", |b| {
        let src = [0xAAu8; 8];
        b.iter_batched(
            || Cursor::new([0u8; 256]),
            |cursor| {
                let mut cursor = black_box(cursor);
                let dst = cursor.buf_mut().unwrap();
                dst[..8].copy_from_slice(black_box(&src));
                cursor.advance_mut(8);
                black_box(cursor);
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// VecWriter benchmarks (growable backing buffer)
// ---------------------------------------------------------------------------

fn bench_vec_writer(c: &mut Criterion) {
    let mut group = c.benchmark_group("vec_writer");

    // Cost of a fresh `VecWriter::new()` (no allocation, just a Vec::new).
    group.bench_function("new", |b| {
        b.iter(|| {
            black_box(VecWriter::new());
        })
    });

    // First `buf_mut()` triggers the `cap.max(256)` reserve — measures the
    // initial allocation that all other writes amortize against.
    group.bench_function("first_buf_mut", |b| {
        b.iter_batched(
            VecWriter::new,
            |writer| {
                let mut writer = black_box(writer);
                black_box(writer.buf_mut());
            },
            BatchSize::SmallInput,
        )
    });

    // `write(&[u8])` for various sizes from a fresh writer (each iteration
    // pays one allocation).
    for &len in &[1usize, 4, 8, 16, 32, 64, 256, 1024] {
        group.bench_with_input(BenchmarkId::new("write_fresh", len), &len, |b, &len| {
            let src = [0xAAu8; 1024];
            b.iter_batched(
                VecWriter::new,
                |mut writer| {
                    let _ = writer.write(black_box(&src[..len])).unwrap();
                    black_box(writer);
                },
                BatchSize::SmallInput,
            )
        });
    }

    // Sequential small writes through the `buf_mut`/`advance_mut` fast path,
    // matching what varint encode does. 8 writes of 9 bytes each (worst-case
    // u64 encoding) into a fresh writer.
    group.bench_function("varint_pattern_8x9", |b| {
        let chunk = [0xAAu8; 9];
        b.iter_batched(
            VecWriter::new,
            |mut writer| {
                for _ in 0..8 {
                    let dst = writer.buf_mut().unwrap();
                    dst[..9].copy_from_slice(black_box(&chunk));
                    writer.advance_mut(9);
                }
                black_box(writer);
            },
            BatchSize::SmallInput,
        )
    });

    // Same pattern but pre-reserved: tests the steady-state cost without
    // any reserve() growth in `buf_mut`.
    group.bench_function("varint_pattern_8x9_prereserved", |b| {
        let chunk = [0xAAu8; 9];
        b.iter_batched(
            || VecWriter::with_capacity(128),
            |mut writer| {
                for _ in 0..8 {
                    let dst = writer.buf_mut().unwrap();
                    dst[..9].copy_from_slice(black_box(&chunk));
                    writer.advance_mut(9);
                }
                black_box(writer);
            },
            BatchSize::SmallInput,
        )
    });

    // Growth pattern: write enough bytes to force multiple `Vec::reserve`
    // doublings (256 -> 512 -> 1024 -> 2048).
    group.bench_function("growth_2048_via_writes_64", |b| {
        let chunk = [0xAAu8; 64];
        b.iter_batched(
            VecWriter::new,
            |mut writer| {
                for _ in 0..32 {
                    let _ = writer.write(black_box(&chunk)).unwrap();
                }
                black_box(writer);
            },
            BatchSize::SmallInput,
        )
    });

    // Same target size, but reserve up front: measures the savings from
    // calling `reserve(n)` before a sequence of writes.
    group.bench_function("growth_2048_prereserved", |b| {
        let chunk = [0xAAu8; 64];
        b.iter_batched(
            || VecWriter::with_capacity(2048),
            |mut writer| {
                for _ in 0..32 {
                    let _ = writer.write(black_box(&chunk)).unwrap();
                }
                black_box(writer);
            },
            BatchSize::SmallInput,
        )
    });

    // `into_inner()` cost — should be near-free, tests that the consume path
    // doesn't add overhead.
    group.bench_function("into_inner", |b| {
        b.iter_batched(
            VecWriter::new,
            |writer| {
                let writer = black_box(writer);
                black_box(writer.into_inner());
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(benches, bench_cursor_read, bench_cursor_write, bench_vec_writer);
criterion_main!(benches);
