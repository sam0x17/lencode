#![cfg(feature = "std")]

use borsh::{BorshDeserialize, BorshSerialize};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use lencode::prelude::*;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use serde::{Deserialize, Serialize};
use std::hint::black_box;
use std::io::Cursor;
use wincode::SchemaReadOwned;
use wincode::io::Cursor as WincodeCursor;
use wincode::{SchemaRead, SchemaWrite};

#[derive(
    Clone,
    Debug,
    PartialEq,
    Serialize,
    Deserialize,
    SchemaWrite,
    SchemaRead,
    Encode,
    Decode,
    BorshSerialize,
    BorshDeserialize,
)]
struct SmallStruct {
    a: u64,
    b: i32,
    c: bool,
    d: [u8; 4],
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Serialize,
    Deserialize,
    SchemaWrite,
    SchemaRead,
    Encode,
    Decode,
    BorshSerialize,
    BorshDeserialize,
)]
struct MediumStruct {
    id: u64,
    flag: bool,
    payload: Vec<u8>,
    numbers: Vec<u64>,
    name: String,
}

fn random_bytes(rng: &mut StdRng, len: usize) -> Vec<u8> {
    (0..len).map(|_| rng.random()).collect()
}

fn bench_enabled(name: &str) -> bool {
    match std::env::var("LENCODE_CODEC_FILTER") {
        Ok(filter) if !filter.is_empty() => name.contains(&filter),
        _ => true,
    }
}

fn random_u64_split(rng: &mut StdRng) -> u64 {
    let half = u64::MAX / 2;
    if rng.random() {
        rng.random_range(0..=half)
    } else {
        rng.random_range((half + 1)..=u64::MAX)
    }
}

fn random_i32_split(rng: &mut StdRng) -> i32 {
    let half = i32::MAX / 2;
    if rng.random() {
        rng.random_range((half + 1)..=i32::MAX)
    } else {
        rng.random_range(i32::MIN..=half)
    }
}

fn make_small(rng: &mut StdRng) -> SmallStruct {
    let mut d = [0u8; 4];
    for byte in d.iter_mut() {
        *byte = rng.random();
    }
    SmallStruct {
        a: random_u64_split(rng),
        b: random_i32_split(rng),
        c: rng.random(),
        d,
    }
}

fn make_medium(
    rng: &mut StdRng,
    payload_len: usize,
    numbers_len: usize,
    compressible: bool,
) -> MediumStruct {
    let payload = if compressible {
        vec![0u8; payload_len]
    } else {
        random_bytes(rng, payload_len)
    };
    let numbers = (0..numbers_len)
        .map(|_| random_u64_split(rng))
        .collect::<Vec<u64>>();
    let name = (0..32)
        .map(|_| (b'a' + (rng.random::<u8>() % 26)) as char)
        .collect::<String>();
    MediumStruct {
        id: random_u64_split(rng),
        flag: rng.random(),
        payload,
        numbers,
        name,
    }
}

#[inline(always)]
fn encode_lencode_into<T: Encode>(value: &T, writer: &mut lencode::io::VecWriter) {
    value.encode_ext(writer, None).unwrap();
}

#[inline(always)]
fn encode_lencode<T: Encode>(value: &T) -> Vec<u8> {
    let mut writer = lencode::io::VecWriter::new();
    encode_lencode_into(value, &mut writer);
    writer.into_inner()
}

#[inline(always)]
fn decode_lencode<T: Decode>(bytes: &[u8]) -> T {
    let mut cursor = lencode::io::Cursor::new(bytes);
    T::decode_ext(&mut cursor, None).unwrap()
}

#[inline(always)]
fn encode_bincode_into<T: Serialize>(value: &T, cursor: &mut Cursor<Vec<u8>>) {
    bincode::serde::encode_into_std_write(value, cursor, bincode::config::standard()).unwrap();
}

#[inline(always)]
fn encode_bincode<T: Serialize>(value: &T) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    encode_bincode_into(value, &mut cursor);
    cursor.into_inner()
}

#[inline(always)]
fn decode_bincode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> T {
    let mut cursor = Cursor::new(bytes);
    bincode::serde::decode_from_std_read(&mut cursor, bincode::config::standard()).unwrap()
}

#[inline(always)]
fn encode_borsh_into<T: BorshSerialize>(value: &T, cursor: &mut Cursor<Vec<u8>>) {
    value.serialize(cursor).unwrap();
}

#[inline(always)]
fn encode_borsh<T: BorshSerialize>(value: &T) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    encode_borsh_into(value, &mut cursor);
    cursor.into_inner()
}

#[inline(always)]
fn decode_borsh<T: BorshDeserialize>(bytes: &[u8]) -> T {
    let mut cursor = Cursor::new(bytes);
    T::deserialize_reader(&mut cursor).unwrap()
}

#[inline(always)]
fn encode_wincode_into<T: SchemaWrite<Src = T>>(value: &T, writer: &mut impl wincode::io::Writer) {
    wincode::serialize_into(writer, value).unwrap();
}

#[inline(always)]
fn encode_wincode<T: SchemaWrite<Src = T>>(value: &T) -> Vec<u8> {
    wincode::serialize(value).unwrap()
}

#[inline(always)]
fn decode_wincode<T>(bytes: &[u8]) -> T
where
    T: SchemaReadOwned<Dst = T>,
{
    wincode::deserialize(bytes).unwrap()
}

fn bench_codec<T>(c: &mut Criterion, name: &str, value: &T)
where
    T: Encode
        + Decode
        + Serialize
        + serde::de::DeserializeOwned
        + BorshSerialize
        + BorshDeserialize
        + SchemaWrite<Src = T>
        + SchemaReadOwned<Dst = T>
        + for<'de> SchemaRead<'de, Dst = T>,
{
    let mut group = c.comparison_benchmark_group(format!("{name}_encode"));
    group.bench_function("bincode", |b| {
        b.iter_batched(
            || Cursor::new(Vec::new()),
            |mut cursor| {
                encode_bincode_into(value, &mut cursor);
                black_box(cursor.into_inner());
            },
            BatchSize::SmallInput,
        )
    });
    group.bench_function("borsh", |b| {
        b.iter_batched(
            || Cursor::new(Vec::new()),
            |mut cursor| {
                encode_borsh_into(value, &mut cursor);
                black_box(cursor.into_inner());
            },
            BatchSize::SmallInput,
        )
    });
    group.bench_function("wincode", |b| {
        b.iter_batched(
            || WincodeCursor::new(Vec::new()),
            |mut cursor| {
                encode_wincode_into(value, &mut cursor);
                black_box(cursor.into_inner());
            },
            BatchSize::SmallInput,
        )
    });
    group.bench_function("lencode", |b| {
        b.iter_batched(
            lencode::io::VecWriter::new,
            |mut writer| {
                encode_lencode_into(value, &mut writer);
                black_box(writer.into_inner());
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();

    let lencode_bytes = encode_lencode(value);
    let bincode_bytes = encode_bincode(value);
    let borsh_bytes = encode_borsh(value);
    let wincode_bytes = encode_wincode(value);

    let mut group = c.comparison_benchmark_group(format!("{name}_decode"));
    group.bench_function("lencode", |b| {
        b.iter(|| black_box(decode_lencode::<T>(&lencode_bytes)))
    });
    group.bench_function("bincode", |b| {
        b.iter(|| black_box(decode_bincode::<T>(&bincode_bytes)))
    });
    group.bench_function("borsh", |b| {
        b.iter(|| black_box(decode_borsh::<T>(&borsh_bytes)))
    });
    group.bench_function("wincode", |b| {
        b.iter(|| black_box(decode_wincode::<T>(&wincode_bytes)))
    });
    group.finish();

    println!(
        "[size] {name}: lencode={} bincode={} borsh={} wincode={}",
        lencode_bytes.len(),
        bincode_bytes.len(),
        borsh_bytes.len(),
        wincode_bytes.len()
    );
}

fn benchmark_regular_codecs(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0xC0DEC0DE);

    if bench_enabled("regular_small_struct") {
        let small = make_small(&mut rng);
        bench_codec(c, "regular_small_struct", &small);
    }

    if bench_enabled("regular_medium_random") {
        let medium_random = make_medium(&mut rng, 512, 8, false);
        bench_codec(c, "regular_medium_random", &medium_random);
    }

    if bench_enabled("regular_medium_compressible") {
        let medium_compressible = make_medium(&mut rng, 512, 8, true);
        bench_codec(c, "regular_medium_compressible", &medium_compressible);
    }

    if bench_enabled("regular_u64") {
        let value_u64: u64 = random_u64_split(&mut rng);
        bench_codec(c, "regular_u64", &value_u64);
    }

    if bench_enabled("regular_bytes_random_256") {
        let bytes_random = random_bytes(&mut rng, 256);
        bench_codec(c, "regular_bytes_random_256", &bytes_random);
    }
}

criterion_group!(benches, benchmark_regular_codecs);
criterion_main!(benches);
