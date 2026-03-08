#![cfg(feature = "std")]

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use lencode::context::{DecoderContext, EncoderContext};
use lencode::diff::{DiffDecoder, DiffEncoder};
use lencode::prelude::*;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::hint::black_box;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a deterministic blob of `len` bytes from a seeded RNG.
fn make_blob(rng: &mut StdRng, len: usize) -> Vec<u8> {
    (0..len).map(|_| rng.random()).collect()
}

/// Mutate `pct`% of bytes at random positions (non-contiguous).
fn scatter_mutate(blob: &mut [u8], rng: &mut StdRng, pct: f64) {
    let n = ((blob.len() as f64) * pct / 100.0).max(1.0) as usize;
    for _ in 0..n {
        let idx = rng.random_range(0..blob.len());
        blob[idx] = blob[idx].wrapping_add(1);
    }
}

/// Mutate a single contiguous region of `count` bytes starting at a random offset.
fn contiguous_mutate(blob: &mut [u8], rng: &mut StdRng, count: usize) {
    let count = count.min(blob.len());
    let start = rng.random_range(0..=blob.len() - count);
    for b in &mut blob[start..start + count] {
        *b = b.wrapping_add(1);
    }
}

/// Encode a blob through the diff encoder and return (encoded_bytes, wire_len).
fn diff_encode(encoder: &mut DiffEncoder, key: u64, data: &[u8]) -> (Vec<u8>, usize) {
    encoder.set_key(key);
    let mut buf = Vec::new();
    let n = encoder.encode_blob(data, &mut buf).unwrap();
    (buf, n)
}

/// Full roundtrip: encode then decode, returning decoded blob.
fn diff_roundtrip(
    encoder: &mut DiffEncoder,
    decoder: &mut DiffDecoder,
    key: u64,
    data: &[u8],
) -> Vec<u8> {
    let (buf, _) = diff_encode(encoder, key, data);
    decoder.set_key(key);
    let mut cursor = Cursor::new(&buf[..]);
    decoder.decode_blob(&mut cursor).unwrap()
}

/// Prime an encoder with an initial blob for a key, returning the encoder.
fn primed_encoder(key: u64, data: &[u8]) -> DiffEncoder {
    let mut enc = DiffEncoder::new();
    enc.set_key(key);
    enc.encode_blob(data, &mut Vec::new()).unwrap();
    enc
}

/// Prime a decoder by encoding + decoding the initial blob.
fn primed_decoder(key: u64, data: &[u8]) -> DiffDecoder {
    let mut enc = DiffEncoder::new();
    let mut dec = DiffDecoder::new();
    diff_roundtrip(&mut enc, &mut dec, key, data);
    dec
}

// ---------------------------------------------------------------------------
// Compression-ratio report (not timed — just prints sizes)
// ---------------------------------------------------------------------------

fn report_compression_ratios(_c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0xD1FF_BEEF);

    println!();
    println!("=== Diff compression ratios ===");
    println!(
        "{:<40} {:>8} {:>8} {:>8} {:>6}",
        "scenario", "blob", "full", "diff", "ratio"
    );
    println!("{}", "-".repeat(74));

    let scenarios: Vec<(&str, usize, Box<dyn Fn(&mut Vec<u8>, &mut StdRng)>)> = vec![
        // Tiny change in 1 KB blob
        (
            "1KB_1byte_change",
            1024,
            Box::new(|blob: &mut Vec<u8>, rng: &mut StdRng| {
                let idx = rng.random_range(0..blob.len());
                blob[idx] ^= 0xFF;
            }),
        ),
        // 1% scattered changes in 4 KB
        (
            "4KB_1pct_scatter",
            4096,
            Box::new(|blob, rng| scatter_mutate(blob, rng, 1.0)),
        ),
        // 5% scattered changes in 4 KB
        (
            "4KB_5pct_scatter",
            4096,
            Box::new(|blob, rng| scatter_mutate(blob, rng, 5.0)),
        ),
        // 10% scattered changes in 4 KB
        (
            "4KB_10pct_scatter",
            4096,
            Box::new(|blob, rng| scatter_mutate(blob, rng, 10.0)),
        ),
        // 25% scattered changes
        (
            "4KB_25pct_scatter",
            4096,
            Box::new(|blob, rng| scatter_mutate(blob, rng, 25.0)),
        ),
        // 50% scattered changes — should fall back to full blob
        (
            "4KB_50pct_scatter",
            4096,
            Box::new(|blob, rng| scatter_mutate(blob, rng, 50.0)),
        ),
        // Contiguous 64-byte patch in 4 KB
        (
            "4KB_contiguous_64B",
            4096,
            Box::new(|blob, rng| contiguous_mutate(blob, rng, 64)),
        ),
        // Contiguous 512-byte patch in 4 KB
        (
            "4KB_contiguous_512B",
            4096,
            Box::new(|blob, rng| contiguous_mutate(blob, rng, 512)),
        ),
        // Append 128 bytes to a 4 KB blob
        (
            "4KB_append_128B",
            4096,
            Box::new(|blob, rng: &mut StdRng| {
                let extra: Vec<u8> = (0..128).map(|_| rng.random()).collect();
                blob.extend_from_slice(&extra);
            }),
        ),
        // Truncate 128 bytes from a 4 KB blob
        (
            "4KB_truncate_128B",
            4096,
            Box::new(|blob, _rng| {
                blob.truncate(blob.len() - 128);
            }),
        ),
        // Identical blob (zero diff)
        (
            "4KB_identical",
            4096,
            Box::new(|_blob, _rng| { /* no change */ }),
        ),
        // Large blob: 1% scattered in 64 KB
        (
            "64KB_1pct_scatter",
            65536,
            Box::new(|blob, rng| scatter_mutate(blob, rng, 1.0)),
        ),
        // Large blob: contiguous 1 KB patch in 64 KB
        (
            "64KB_contiguous_1KB",
            65536,
            Box::new(|blob, rng| contiguous_mutate(blob, rng, 1024)),
        ),
        // Large blob: 10% scattered in 64 KB
        (
            "64KB_10pct_scatter",
            65536,
            Box::new(|blob, rng| scatter_mutate(blob, rng, 10.0)),
        ),
        // Large blob: identical 64 KB
        (
            "64KB_identical",
            65536,
            Box::new(|_blob, _rng| {}),
        ),
        // Very large blob: 256 KB with 0.1% changes
        (
            "256KB_0.1pct_scatter",
            262144,
            Box::new(|blob, rng| scatter_mutate(blob, rng, 0.1)),
        ),
    ];

    for (name, size, mutator) in &scenarios {
        let mut enc = DiffEncoder::new();
        let key = 1u64;

        let original = make_blob(&mut rng, *size);

        // Prime with initial full blob
        let (full_buf, _) = diff_encode(&mut enc, key, &original);
        let full_size = full_buf.len();

        // Mutate and diff-encode
        let mut modified = original.clone();
        mutator(&mut modified, &mut rng);
        let (diff_buf, _) = diff_encode(&mut enc, key, &modified);
        let diff_size = diff_buf.len();

        let ratio = diff_size as f64 / full_size as f64;
        println!(
            "{:<40} {:>8} {:>8} {:>8} {:>5.1}%",
            name,
            modified.len(),
            full_size,
            diff_size,
            ratio * 100.0,
        );

        // Verify roundtrip correctness
        let mut dec = primed_decoder(key, &original);
        dec.set_key(key);
        let mut cursor = Cursor::new(&diff_buf[..]);
        let decoded = dec.decode_blob(&mut cursor).unwrap();
        assert_eq!(decoded, modified, "roundtrip failed for {name}");
    }
    println!();
}

// ---------------------------------------------------------------------------
// Encode throughput benchmarks
// ---------------------------------------------------------------------------

fn bench_diff_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("diff_encode");
    let mut rng = StdRng::seed_from_u64(0xD1FF_0001);

    struct Case {
        name_size: &'static str,
        name_variant: &'static str,
        original: Vec<u8>,
        modified: Vec<u8>,
    }

    let mut cases: Vec<Case> = Vec::new();

    // 4 KB, 1% scatter
    {
        let original = make_blob(&mut rng, 4096);
        let mut modified = original.clone();
        scatter_mutate(&mut modified, &mut rng, 1.0);
        cases.push(Case {
            name_size: "4KB",
            name_variant: "1pct_scatter",
            original,
            modified,
        });
    }
    // 4 KB, 10% scatter
    {
        let original = make_blob(&mut rng, 4096);
        let mut modified = original.clone();
        scatter_mutate(&mut modified, &mut rng, 10.0);
        cases.push(Case {
            name_size: "4KB",
            name_variant: "10pct_scatter",
            original,
            modified,
        });
    }
    // 4 KB, contiguous 64B
    {
        let original = make_blob(&mut rng, 4096);
        let mut modified = original.clone();
        contiguous_mutate(&mut modified, &mut rng, 64);
        cases.push(Case {
            name_size: "4KB",
            name_variant: "contiguous_64B",
            original,
            modified,
        });
    }
    // 4 KB identical
    {
        let original = make_blob(&mut rng, 4096);
        cases.push(Case {
            name_size: "4KB",
            name_variant: "identical",
            original: original.clone(),
            modified: original,
        });
    }
    // 64 KB, 1% scatter
    {
        let original = make_blob(&mut rng, 65536);
        let mut modified = original.clone();
        scatter_mutate(&mut modified, &mut rng, 1.0);
        cases.push(Case {
            name_size: "64KB",
            name_variant: "1pct_scatter",
            original,
            modified,
        });
    }
    // 64 KB, contiguous 1 KB
    {
        let original = make_blob(&mut rng, 65536);
        let mut modified = original.clone();
        contiguous_mutate(&mut modified, &mut rng, 1024);
        cases.push(Case {
            name_size: "64KB",
            name_variant: "contiguous_1KB",
            original,
            modified,
        });
    }
    // 256 KB, 0.1% scatter
    {
        let original = make_blob(&mut rng, 262144);
        let mut modified = original.clone();
        scatter_mutate(&mut modified, &mut rng, 0.1);
        cases.push(Case {
            name_size: "256KB",
            name_variant: "0.1pct_scatter",
            original,
            modified,
        });
    }

    for case in &cases {
        group.bench_with_input(
            BenchmarkId::new(case.name_size, case.name_variant),
            &case,
            |b, case| {
                b.iter_batched(
                    || primed_encoder(1, &case.original),
                    |mut enc| {
                        enc.set_key(1);
                        let mut buf = Vec::new();
                        black_box(enc.encode_blob(&case.modified, &mut buf).unwrap());
                        black_box(buf);
                    },
                    criterion::BatchSize::SmallInput,
                )
            },
        );
    }

    // Full blob (no prior state — baseline)
    {
        let blob = make_blob(&mut rng, 4096);
        group.bench_function(BenchmarkId::new("4KB", "full_blob_baseline"), |b| {
            b.iter(|| {
                let mut enc = DiffEncoder::new();
                enc.set_key(1);
                let mut buf = Vec::new();
                black_box(enc.encode_blob(&blob, &mut buf).unwrap());
                black_box(buf);
            })
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Decode throughput benchmarks
// ---------------------------------------------------------------------------

fn bench_diff_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("diff_decode");
    let mut rng = StdRng::seed_from_u64(0xD1FF_0002);

    struct PreparedCase {
        name_size: &'static str,
        name_variant: &'static str,
        original: Vec<u8>,
        diff_buf: Vec<u8>,
    }

    let mut cases: Vec<PreparedCase> = Vec::new();

    let mut prepare = |rng: &mut StdRng,
                       name_size: &'static str,
                       name_variant: &'static str,
                       size: usize,
                       mutator: &dyn Fn(&mut Vec<u8>, &mut StdRng)| {
        let original = make_blob(rng, size);
        let mut modified = original.clone();
        mutator(&mut modified, rng);

        let mut enc = primed_encoder(1, &original);
        let (diff_buf, _) = diff_encode(&mut enc, 1, &modified);

        cases.push(PreparedCase {
            name_size,
            name_variant,
            original,
            diff_buf,
        });
    };

    prepare(&mut rng, "4KB", "1pct_scatter", 4096, &|blob, rng| {
        scatter_mutate(blob, rng, 1.0)
    });
    prepare(&mut rng, "4KB", "contiguous_64B", 4096, &|blob, rng| {
        contiguous_mutate(blob, rng, 64)
    });
    prepare(&mut rng, "4KB", "identical", 4096, &|_, _| {});
    prepare(&mut rng, "4KB", "10pct_scatter", 4096, &|blob, rng| {
        scatter_mutate(blob, rng, 10.0)
    });
    prepare(&mut rng, "64KB", "1pct_scatter", 65536, &|blob, rng| {
        scatter_mutate(blob, rng, 1.0)
    });
    prepare(
        &mut rng,
        "64KB",
        "contiguous_1KB",
        65536,
        &|blob, rng| contiguous_mutate(blob, rng, 1024),
    );
    prepare(
        &mut rng,
        "256KB",
        "0.1pct_scatter",
        262144,
        &|blob, rng| scatter_mutate(blob, rng, 0.1),
    );

    for case in &cases {
        group.bench_with_input(
            BenchmarkId::new(case.name_size, case.name_variant),
            &case,
            |b, case| {
                b.iter_batched(
                    || primed_decoder(1, &case.original),
                    |mut dec| {
                        dec.set_key(1);
                        let mut cursor = Cursor::new(&case.diff_buf[..]);
                        black_box(dec.decode_blob(&mut cursor).unwrap());
                    },
                    criterion::BatchSize::SmallInput,
                )
            },
        );
    }

    // Full blob decode baseline
    {
        let blob = make_blob(&mut rng, 4096);
        let mut enc = DiffEncoder::new();
        let mut buf = Vec::new();
        enc.set_key(1);
        enc.encode_blob(&blob, &mut buf).unwrap();

        group.bench_function(BenchmarkId::new("4KB", "full_blob_baseline"), |b| {
            b.iter(|| {
                let mut dec = DiffDecoder::new();
                dec.set_key(1);
                let mut cursor = Cursor::new(&buf[..]);
                black_box(dec.decode_blob(&mut cursor).unwrap());
            })
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Roundtrip via Vec<u8> Encode/Decode (integration with context API)
// ---------------------------------------------------------------------------

fn bench_diff_roundtrip_via_context(c: &mut Criterion) {
    let mut group = c.benchmark_group("diff_roundtrip_context");
    let mut rng = StdRng::seed_from_u64(0xD1FF_0003);

    // 4 KB, 1% scatter — encode_ext path
    let original: Vec<u8> = make_blob(&mut rng, 4096);
    let mut modified = original.clone();
    scatter_mutate(&mut modified, &mut rng, 1.0);

    group.bench_function("4KB_1pct_encode_ext", |b| {
        b.iter_batched(
            || {
                let mut ctx = EncoderContext::with_diff();
                ctx.diff.as_mut().unwrap().set_key(1);
                let mut prime_buf = Vec::new();
                original
                    .encode_ext(&mut prime_buf, Some(&mut ctx))
                    .unwrap();
                ctx
            },
            |mut ctx| {
                ctx.diff.as_mut().unwrap().set_key(1);
                let mut buf = Vec::new();
                black_box(
                    modified
                        .encode_ext(&mut buf, Some(&mut ctx))
                        .unwrap(),
                );
                black_box(buf);
            },
            criterion::BatchSize::SmallInput,
        )
    });

    // Prepare the diff-encoded buffer for decode benchmark
    let diff_buf = {
        let mut ctx = EncoderContext::with_diff();
        ctx.diff.as_mut().unwrap().set_key(1);
        let mut prime_buf = Vec::new();
        original
            .encode_ext(&mut prime_buf, Some(&mut ctx))
            .unwrap();
        ctx.diff.as_mut().unwrap().set_key(1);
        let mut buf = Vec::new();
        modified.encode_ext(&mut buf, Some(&mut ctx)).unwrap();
        buf
    };

    // Prepare the prime buffer for decoder setup
    let prime_buf = {
        let mut ctx = EncoderContext::with_diff();
        ctx.diff.as_mut().unwrap().set_key(1);
        let mut buf = Vec::new();
        original.encode_ext(&mut buf, Some(&mut ctx)).unwrap();
        buf
    };

    group.bench_function("4KB_1pct_decode_ext", |b| {
        b.iter_batched(
            || {
                let mut ctx = DecoderContext::with_diff();
                ctx.diff.as_mut().unwrap().set_key(1);
                let _: Vec<u8> =
                    Vec::decode_ext(&mut Cursor::new(&prime_buf[..]), Some(&mut ctx)).unwrap();
                ctx
            },
            |mut ctx| {
                ctx.diff.as_mut().unwrap().set_key(1);
                let mut cursor = Cursor::new(&diff_buf[..]);
                black_box(Vec::<u8>::decode_ext(&mut cursor, Some(&mut ctx)).unwrap());
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Multi-key streaming benchmark (simulates real usage with multiple keys)
// ---------------------------------------------------------------------------

fn bench_diff_multikey(c: &mut Criterion) {
    let mut group = c.benchmark_group("diff_multikey");
    let mut rng = StdRng::seed_from_u64(0xD1FF_0004);

    let num_keys = 16u64;
    let blob_size = 2048;

    // Build initial blobs for each key
    let originals: Vec<Vec<u8>> = (0..num_keys).map(|_| make_blob(&mut rng, blob_size)).collect();

    // Build modified versions (1% scatter each)
    let modifieds: Vec<Vec<u8>> = originals
        .iter()
        .map(|orig| {
            let mut m = orig.clone();
            scatter_mutate(&mut m, &mut rng, 1.0);
            m
        })
        .collect();

    group.bench_function("16keys_2KB_1pct_encode", |b| {
        b.iter_batched(
            || {
                let mut enc = DiffEncoder::with_capacity(num_keys as usize);
                for (i, orig) in originals.iter().enumerate() {
                    enc.set_key(i as u64);
                    enc.encode_blob(orig, &mut Vec::new()).unwrap();
                }
                enc
            },
            |mut enc| {
                let mut buf = Vec::new();
                for (i, modified) in modifieds.iter().enumerate() {
                    enc.set_key(i as u64);
                    buf.clear();
                    black_box(enc.encode_blob(modified, &mut buf).unwrap());
                }
                black_box(buf);
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("16keys_2KB_1pct_roundtrip", |b| {
        b.iter_batched(
            || {
                let mut enc = DiffEncoder::with_capacity(num_keys as usize);
                let mut dec = DiffDecoder::with_capacity(num_keys as usize);
                for (i, orig) in originals.iter().enumerate() {
                    let key = i as u64;
                    diff_roundtrip(&mut enc, &mut dec, key, orig);
                }
                (enc, dec)
            },
            |(mut enc, mut dec)| {
                for (i, modified) in modifieds.iter().enumerate() {
                    let key = i as u64;
                    black_box(diff_roundtrip(&mut enc, &mut dec, key, modified));
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Successive diffs — measures how encoding degrades over many small mutations
// ---------------------------------------------------------------------------

fn bench_diff_successive(c: &mut Criterion) {
    let mut group = c.benchmark_group("diff_successive");
    let mut rng = StdRng::seed_from_u64(0xD1FF_0005);

    let blob_size = 4096;
    let num_iterations = 20;

    // Build a chain of 20 successive 1% mutations
    let initial = make_blob(&mut rng, blob_size);
    let mut chain: Vec<Vec<u8>> = Vec::with_capacity(num_iterations + 1);
    chain.push(initial);
    for _ in 0..num_iterations {
        let mut next = chain.last().unwrap().clone();
        scatter_mutate(&mut next, &mut rng, 1.0);
        chain.push(next);
    }

    group.bench_function("4KB_20x_1pct_encode_chain", |b| {
        b.iter_batched(
            || {
                let mut enc = DiffEncoder::new();
                enc.set_key(1);
                enc.encode_blob(&chain[0], &mut Vec::new()).unwrap();
                enc
            },
            |mut enc| {
                for blob in &chain[1..] {
                    enc.set_key(1);
                    let mut buf = Vec::new();
                    black_box(enc.encode_blob(blob, &mut buf).unwrap());
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("4KB_20x_1pct_roundtrip_chain", |b| {
        b.iter_batched(
            || {
                let mut enc = DiffEncoder::new();
                let mut dec = DiffDecoder::new();
                diff_roundtrip(&mut enc, &mut dec, 1, &chain[0]);
                (enc, dec)
            },
            |(mut enc, mut dec)| {
                for blob in &chain[1..] {
                    black_box(diff_roundtrip(&mut enc, &mut dec, 1, blob));
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });

    // Report cumulative wire sizes for the chain
    {
        let mut enc = DiffEncoder::new();
        enc.set_key(1);
        enc.encode_blob(&chain[0], &mut Vec::new()).unwrap();

        let mut total_wire = 0usize;
        let mut total_raw = 0usize;
        for blob in &chain[1..] {
            enc.set_key(1);
            let mut buf = Vec::new();
            enc.encode_blob(blob, &mut buf).unwrap();
            total_wire += buf.len();
            total_raw += blob.len();
        }
        println!(
            "[successive] 4KB x {num_iterations} diffs: total_wire={total_wire} total_raw={total_raw} ratio={:.1}%",
            total_wire as f64 / total_raw as f64 * 100.0
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    report_compression_ratios,
    bench_diff_encode,
    bench_diff_decode,
    bench_diff_roundtrip_via_context,
    bench_diff_multikey,
    bench_diff_successive,
);
criterion_main!(benches);
