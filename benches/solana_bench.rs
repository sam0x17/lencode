#![cfg(all(feature = "solana", feature = "std"))]

use borsh::{BorshDeserialize, BorshSerialize};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use lencode::prelude::*;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use solana_message::compiled_instruction::CompiledInstruction;
use solana_pubkey::Pubkey;
use std::hint::black_box;
use std::io::Cursor;
use wincode::io::{ReadError as WincodeReadError, ReadResult as WincodeReadResult};
use wincode::io::{Reader as WincodeReader, WriteError as WincodeWriteError};
use wincode::io::{WriteResult as WincodeWriteResult, Writer as WincodeWriter};
use wincode::{SchemaRead, SchemaReadOwned, SchemaWrite};

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    SchemaWrite,
    SchemaRead,
    BorshSerialize,
    BorshDeserialize,
)]
#[repr(transparent)]
struct BenchPubkey([u8; 32]);

impl From<Pubkey> for BenchPubkey {
    fn from(value: Pubkey) -> Self {
        Self(value.to_bytes())
    }
}

impl Pack for BenchPubkey {
    fn pack(&self, writer: &mut impl Write) -> Result<usize> {
        self.0.pack(writer)
    }

    fn unpack(reader: &mut impl Read) -> Result<Self> {
        Ok(Self(<[u8; 32]>::unpack(reader)?))
    }
}

impl DedupeEncodeable for BenchPubkey {}
impl DedupeDecodeable for BenchPubkey {}

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
struct BenchCompiledInstruction {
    program_id_index: u8,
    #[serde(with = "solana_short_vec")]
    #[wincode(with = "wincode::containers::Vec<_, wincode::len::ShortU16Len>")]
    accounts: Vec<u8>,
    #[serde(with = "solana_short_vec")]
    #[wincode(with = "wincode::containers::Vec<_, wincode::len::ShortU16Len>")]
    data: Vec<u8>,
}

impl From<&CompiledInstruction> for BenchCompiledInstruction {
    fn from(value: &CompiledInstruction) -> Self {
        Self {
            program_id_index: value.program_id_index,
            accounts: value.accounts.clone(),
            data: value.data.clone(),
        }
    }
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
struct BenchMessage {
    #[serde(with = "solana_short_vec")]
    #[wincode(with = "wincode::containers::Vec<_, wincode::len::ShortU16Len>")]
    account_keys: Vec<BenchPubkey>,
    recent_blockhash: [u8; 32],
    #[serde(with = "solana_short_vec")]
    #[wincode(with = "wincode::containers::Vec<_, wincode::len::ShortU16Len>")]
    instructions: Vec<BenchCompiledInstruction>,
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
fn encode_lencode_dedupe_into<T: Encode>(
    value: &T,
    encoder: &mut DedupeEncoder,
    cursor: &mut Cursor<Vec<u8>>,
) {
    value.encode_ext(cursor, Some(encoder)).unwrap();
}

#[inline(always)]
fn encode_lencode_dedupe<T: Encode>(value: &T, encoder: &mut DedupeEncoder) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    encode_lencode_dedupe_into(value, encoder, &mut cursor);
    cursor.into_inner()
}

#[inline(always)]
fn decode_lencode<T: Decode>(bytes: &[u8]) -> T {
    let mut cursor = Cursor::new(bytes);
    T::decode_ext(&mut cursor, None).unwrap()
}

#[inline(always)]
fn decode_lencode_dedupe<T: Decode>(bytes: &[u8], decoder: &mut DedupeDecoder) -> T {
    let mut cursor = Cursor::new(bytes);
    T::decode_ext(&mut cursor, Some(decoder)).unwrap()
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

fn make_pubkeys(rng: &mut StdRng, count: usize) -> Vec<BenchPubkey> {
    (0..count)
        .map(|_| {
            let bytes: [u8; 32] = rng.random();
            let pubkey = Pubkey::new_from_array(bytes);
            BenchPubkey::from(pubkey)
        })
        .collect()
}

fn make_pubkeys_with_duplicates(rng: &mut StdRng, count: usize) -> Vec<BenchPubkey> {
    let unique_count = count / 2;
    let mut unique = make_pubkeys(rng, unique_count);
    let mut dupes = (0..(count - unique_count))
        .map(|_| {
            let idx = rng.random_range(0..unique.len());
            unique[idx].clone()
        })
        .collect::<Vec<_>>();
    unique.append(&mut dupes);
    unique.shuffle(rng);
    unique
}

fn make_instructions(
    rng: &mut StdRng,
    count: usize,
    accounts_len: usize,
    data_len: usize,
) -> Vec<BenchCompiledInstruction> {
    (0..count)
        .map(|i| {
            let accounts = (0..accounts_len)
                .map(|_| rng.random_range(0..64) as u8)
                .collect::<Vec<u8>>();
            let data = (0..data_len).map(|_| rng.random()).collect::<Vec<u8>>();
            let ix = CompiledInstruction {
                program_id_index: (i % 16) as u8,
                accounts,
                data,
            };
            BenchCompiledInstruction::from(&ix)
        })
        .collect()
}

fn make_message(rng: &mut StdRng, key_count: usize, ix_count: usize) -> BenchMessage {
    let account_keys = make_pubkeys(rng, key_count);
    let recent_blockhash: [u8; 32] = rng.random();
    let instructions = make_instructions(rng, ix_count, 8, 96);
    BenchMessage {
        account_keys,
        recent_blockhash,
        instructions,
    }
}

fn bench_pubkey_vec(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0xA11CE);
    let pubkeys = make_pubkeys(&mut rng, 1024);
    bench_codec(c, "solana_pubkey_vec_1k", &pubkeys);
}

fn bench_pubkey_vec_dupes(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0xD00D);
    let pubkeys = make_pubkeys_with_duplicates(&mut rng, 1024);

    let mut group = c.comparison_benchmark_group("solana_pubkey_vec_50pct_dupes_encode");
    group.bench_function("lencode", |b| {
        b.iter_batched(
            || Cursor::new(Vec::new()),
            |mut cursor| {
                encode_lencode_into(&pubkeys, &mut cursor);
                black_box(cursor.into_inner());
            },
            BatchSize::SmallInput,
        )
    });
    group.bench_function("lencode_dedupe", |b| {
        b.iter_batched(
            || {
                (
                    Cursor::new(Vec::new()),
                    DedupeEncoder::with_capacity(2048, 1),
                )
            },
            |(mut cursor, mut encoder)| {
                encode_lencode_dedupe_into(&pubkeys, &mut encoder, &mut cursor);
                black_box(cursor.into_inner());
            },
            BatchSize::SmallInput,
        )
    });
    group.bench_function("bincode", |b| {
        b.iter_batched(
            || Cursor::new(Vec::new()),
            |mut cursor| {
                encode_bincode_into(&pubkeys, &mut cursor);
                black_box(cursor.into_inner());
            },
            BatchSize::SmallInput,
        )
    });
    group.bench_function("borsh", |b| {
        b.iter_batched(
            || Cursor::new(Vec::new()),
            |mut cursor| {
                encode_borsh_into(&pubkeys, &mut cursor);
                black_box(cursor.into_inner());
            },
            BatchSize::SmallInput,
        )
    });
    group.bench_function("wincode", |b| {
        b.iter_batched(
            || Cursor::new(Vec::new()),
            |mut cursor| {
                encode_wincode_into(&pubkeys, &mut cursor);
                black_box(cursor.into_inner());
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();

    let lencode_bytes = encode_lencode(&pubkeys);
    let lencode_dedupe_bytes = {
        let mut encoder = DedupeEncoder::with_capacity(2048, 1);
        encode_lencode_dedupe(&pubkeys, &mut encoder)
    };
    let bincode_bytes = encode_bincode(&pubkeys);
    let borsh_bytes = encode_borsh(&pubkeys);
    let wincode_bytes = encode_wincode(&pubkeys);

    let mut group = c.comparison_benchmark_group("solana_pubkey_vec_50pct_dupes_decode");
    group.bench_function("lencode", |b| {
        b.iter(|| black_box(decode_lencode::<Vec<BenchPubkey>>(&lencode_bytes)))
    });
    group.bench_function("lencode_dedupe", |b| {
        b.iter_batched(
            || DedupeDecoder::with_capacity(2048),
            |mut decoder| {
                black_box(decode_lencode_dedupe::<Vec<BenchPubkey>>(
                    &lencode_dedupe_bytes,
                    &mut decoder,
                ))
            },
            BatchSize::SmallInput,
        )
    });
    group.bench_function("bincode", |b| {
        b.iter(|| black_box(decode_bincode::<Vec<BenchPubkey>>(&bincode_bytes)))
    });
    group.bench_function("borsh", |b| {
        b.iter(|| black_box(decode_borsh::<Vec<BenchPubkey>>(&borsh_bytes)))
    });
    group.bench_function("wincode", |b| {
        b.iter(|| black_box(decode_wincode::<Vec<BenchPubkey>>(&wincode_bytes)))
    });
    group.finish();
}

fn bench_message(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0xBEEF);
    let message = make_message(&mut rng, 128, 64);
    bench_codec(c, "solana_message", &message);
}

criterion_group!(
    benches,
    bench_pubkey_vec,
    bench_pubkey_vec_dupes,
    bench_message
);
criterion_main!(benches);
