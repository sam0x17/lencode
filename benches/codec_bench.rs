#![cfg(feature = "std")]

use borsh::{BorshDeserialize, BorshSerialize};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use lencode::prelude::*;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::hint::black_box;
use std::io::Cursor;
use wincode::SchemaReadOwned;
use wincode::io::{ReadError as WincodeReadError, ReadResult as WincodeReadResult};
use wincode::io::{Reader as WincodeReader, WriteError as WincodeWriteError};
use wincode::io::{WriteResult as WincodeWriteResult, Writer as WincodeWriter};
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
    d: [u8; 32],
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

fn make_small(rng: &mut StdRng) -> SmallStruct {
    let mut d = [0u8; 32];
    for byte in d.iter_mut() {
        *byte = rng.random();
    }
    SmallStruct {
        a: rng.random(),
        b: rng.random(),
        c: rng.random(),
        d,
    }
}

fn make_medium(rng: &mut StdRng, payload_len: usize, compressible: bool) -> MediumStruct {
    let payload = if compressible {
        vec![0u8; payload_len]
    } else {
        random_bytes(rng, payload_len)
    };
    let numbers = (0..512).map(|_| rng.random()).collect::<Vec<u64>>();
    let name = (0..32)
        .map(|_| (b'a' + (rng.random::<u8>() % 26)) as char)
        .collect::<String>();
    MediumStruct {
        id: rng.random(),
        flag: rng.random(),
        payload,
        numbers,
        name,
    }
}

struct WincodeStdCursorWriter<'a> {
    cursor: &'a mut Cursor<Vec<u8>>,
}

impl<'a> WincodeWriter for WincodeStdCursorWriter<'a> {
    type Trusted<'b>
        = WincodeStdCursorWriter<'b>
    where
        Self: 'b;

    #[inline(always)]
    fn write(&mut self, src: &[u8]) -> WincodeWriteResult<()> {
        use std::io::Write as _;
        self.cursor
            .write_all(src)
            .map_err(WincodeWriteError::from)?;
        Ok(())
    }

    #[inline(always)]
    unsafe fn as_trusted_for(&mut self, n_bytes: usize) -> WincodeWriteResult<Self::Trusted<'_>> {
        self.cursor.get_mut().reserve(n_bytes);
        Ok(WincodeStdCursorWriter {
            cursor: &mut *self.cursor,
        })
    }
}

struct WincodeStdCursorReader<'a> {
    cursor: Cursor<&'a [u8]>,
}

impl<'a> WincodeStdCursorReader<'a> {
    #[inline(always)]
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            cursor: Cursor::new(bytes),
        }
    }
}

impl<'a> WincodeReader<'a> for WincodeStdCursorReader<'a> {
    type Trusted<'b>
        = WincodeStdCursorReader<'a>
    where
        Self: 'b;

    #[inline(always)]
    fn fill_buf(&mut self, n_bytes: usize) -> WincodeReadResult<&[u8]> {
        let pos = self.cursor.position() as usize;
        let slice = self.cursor.get_ref();
        let end = (pos + n_bytes).min(slice.len());
        Ok(&slice[pos..end])
    }

    #[inline(always)]
    fn borrow_exact(&mut self, len: usize) -> WincodeReadResult<&'a [u8]> {
        let pos = self.cursor.position() as usize;
        let slice = *self.cursor.get_ref();
        let end = pos + len;
        if end > slice.len() {
            return Err(WincodeReadError::ReadSizeLimit(len));
        }
        self.cursor.set_position(end as u64);
        Ok(&slice[pos..end])
    }

    #[inline(always)]
    fn borrow_exact_mut(&mut self, _len: usize) -> WincodeReadResult<&'a mut [u8]> {
        Err(WincodeReadError::UnsupportedZeroCopy)
    }

    #[inline(always)]
    unsafe fn consume_unchecked(&mut self, amt: usize) {
        let pos = self.cursor.position() as usize;
        self.cursor.set_position((pos + amt) as u64);
    }

    #[inline(always)]
    fn consume(&mut self, amt: usize) -> WincodeReadResult<()> {
        let pos = self.cursor.position() as usize;
        let slice = self.cursor.get_ref();
        let end = pos + amt;
        if end > slice.len() {
            return Err(WincodeReadError::ReadSizeLimit(amt));
        }
        self.cursor.set_position(end as u64);
        Ok(())
    }

    #[inline(always)]
    unsafe fn as_trusted_for(&mut self, n_bytes: usize) -> WincodeReadResult<Self::Trusted<'_>> {
        let pos = self.cursor.position() as usize;
        let slice = *self.cursor.get_ref();
        let end = pos + n_bytes;
        if end > slice.len() {
            return Err(WincodeReadError::ReadSizeLimit(n_bytes));
        }
        self.cursor.set_position(end as u64);
        Ok(WincodeStdCursorReader {
            cursor: Cursor::new(&slice[pos..end]),
        })
    }
}

#[inline(always)]
fn encode_lencode_into<T: Encode>(value: &T, cursor: &mut Cursor<Vec<u8>>) {
    value.encode_ext(cursor, None).unwrap();
}

#[inline(always)]
fn encode_lencode<T: Encode>(value: &T) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    encode_lencode_into(value, &mut cursor);
    cursor.into_inner()
}

#[inline(always)]
fn decode_lencode<T: Decode>(bytes: &[u8]) -> T {
    let mut cursor = Cursor::new(bytes);
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
fn encode_wincode_into<T: SchemaWrite<Src = T>>(value: &T, cursor: &mut Cursor<Vec<u8>>) {
    let mut writer = WincodeStdCursorWriter { cursor };
    wincode::serialize_into(&mut writer, value).unwrap();
}

#[inline(always)]
fn encode_wincode<T: SchemaWrite<Src = T>>(value: &T) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    encode_wincode_into(value, &mut cursor);
    cursor.into_inner()
}

#[inline(always)]
fn decode_wincode<T>(bytes: &[u8]) -> T
where
    T: SchemaReadOwned<Dst = T>,
{
    let mut cursor = WincodeStdCursorReader::new(bytes);
    wincode::deserialize_from(&mut cursor).unwrap()
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
            || Cursor::new(Vec::new()),
            |mut cursor| {
                encode_wincode_into(value, &mut cursor);
                black_box(cursor.into_inner());
            },
            BatchSize::SmallInput,
        )
    });
    group.bench_function("lencode", |b| {
        b.iter_batched(
            || Cursor::new(Vec::new()),
            |mut cursor| {
                encode_lencode_into(value, &mut cursor);
                black_box(cursor.into_inner());
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
}

fn benchmark_regular_codecs(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0xC0DEC0DE);

    let small = make_small(&mut rng);
    bench_codec(c, "regular_small_struct", &small);

    let medium_random = make_medium(&mut rng, 64 * 1024, false);
    bench_codec(c, "regular_medium_random", &medium_random);

    let medium_compressible = make_medium(&mut rng, 64 * 1024, true);
    bench_codec(c, "regular_medium_compressible", &medium_compressible);

    let vec_u64 = (0..2048).map(|_| rng.random()).collect::<Vec<u64>>();
    bench_codec(c, "regular_vec_u64_2k", &vec_u64);

    let bytes_random = random_bytes(&mut rng, 64 * 1024);
    bench_codec(c, "regular_bytes_random_64k", &bytes_random);

    let bytes_zero = vec![0u8; 64 * 1024];
    bench_codec(c, "regular_bytes_zero_64k", &bytes_zero);
}

criterion_group!(benches, benchmark_regular_codecs);
criterion_main!(benches);
