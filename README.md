# 📦 lencode

[![Crates.io](https://img.shields.io/crates/v/lencode.svg)](https://crates.io/crates/lencode)
[![docs.rs](https://docs.rs/lencode/badge.svg)](https://docs.rs/lencode)
[![CI](https://github.com/sam0x17/lencode/actions/workflows/ci.yaml/badge.svg?branch=main)](https://github.com/sam0x17/lencode/actions/workflows/ci.yaml)
[![Big Endian CI](https://github.com/sam0x17/lencode/actions/workflows/big-endian.yml/badge.svg?branch=main)](https://github.com/sam0x17/lencode/actions/workflows/big-endian.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Compact, fast binary encoding with varints, optional deduplication, and opportunistic zstd compression for bytes and strings. `no_std` by default, with an opt‑in `std` feature.

## Highlights

- Fast varints: efficient for small and large integers
- Optional deduplication: replace repeats with compact IDs for supported types
- Bytes/strings compression: flagged header + zstd when smaller; high‑entropy data is detected and skipped automatically
- Bulk encoding: `Vec<T>` of fixed‑size types (e.g. `[u8; 32]`) are encoded/decoded via bulk `memcpy`, not per‑element
- no_std + alloc: works without `std` (uses `zstd-safe`)
- Derive macros: `#[derive(Encode, Decode)]` for your types, `#[derive(Pack)]` for dedupe/bulk types
- Solana support: feature `solana` adds v2/v3 SDK types
- Big-endian ready: CI runs tests on s390x

## Install

```toml
[dependencies]
lencode = "0.1"

# With standard library types (e.g., Cow)
lencode = { version = "0.1", features = ["std"] }

# With Solana type support (implies std)
lencode = { version = "0.1", features = ["solana"] }
```

## Quick start

### Derive and round‑trip

```rust
use lencode::prelude::*;

#[derive(Encode, Decode, PartialEq, Debug)]
struct Point { x: u64, y: u64 }

let p = Point { x: 3, y: 5 };
let mut buf = Vec::new();
encode(&p, &mut buf)?;
let q: Point = decode(&mut Cursor::new(&buf))?;
assert_eq!(p, q);
```

### Collections and primitives

```rust
use lencode::prelude::*;

let values: Vec<u128> = (0..10).collect();
let mut buf = Vec::new();
encode(&values, &mut buf)?;
let rt: Vec<u128> = decode(&mut Cursor::new(&buf))?;
assert_eq!(values, rt);
```

### Deduplication (optional)

To benefit from deduplication for your own types, implement `Pack` and the marker traits and pass the encoder/decoder via `encode_ext`/`decode_ext`.

```rust
use lencode::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
struct MyId(u32);

impl Pack for MyId {
    fn pack(&self, w: &mut impl Write) -> Result<usize> { self.0.pack(w) }
    fn unpack(r: &mut impl Read) -> Result<Self> { Ok(Self(u32::unpack(r)?)) }
}
impl DedupeEncodeable for MyId {}
impl DedupeDecodeable for MyId {}

let vals = vec![MyId(42), MyId(7), MyId(42), MyId(7), MyId(42)];

// Encode with deduplication enabled
let mut enc = DedupeEncoder::new();
let mut buf = Vec::new();
encode_ext(&vals, &mut buf, Some(&mut enc))?;

// Decode with deduplication enabled
let mut dec = DedupeDecoder::new();
let roundtrip: Vec<MyId> = decode_ext(&mut Cursor::new(&buf), Some(&mut dec))?;
assert_eq!(roundtrip, vals);
```

### Compact bytes and strings

`&[u8]`, `Vec<u8]`, `VecDeque<u8]`, `&str`, and `String` use a compact flagged header: `varint((payload_len << 1) | flag) + payload`.

- `flag = 0` → raw bytes/UTF‑8
- `flag = 1` → zstd frame (original size stored inside the frame)

The encoder picks whichever is smaller per value. High‑entropy data (random bytes, encrypted content) is detected via a fast entropy check and skips compression entirely.

### Bulk encoding for fixed‑size types

`Vec<T>` where `T` has a fixed‑size wire representation (e.g. `[u8; 32]`, or `#[repr(transparent)]` newtypes over byte arrays) is encoded and decoded via bulk `memcpy` rather than per‑element iteration. This is handled automatically through `Encode::encode_slice` / `Decode::decode_vec` and their `Pack` counterparts `Pack::pack_slice` / `Pack::unpack_vec`.

Custom `Pack` types can opt in by overriding `pack_slice` and `unpack_vec`, or by using `#[derive(Pack)]` on a `#[repr(transparent)]` single‑field struct, which generates the bulk overrides automatically:

```rust
use lencode::prelude::*;

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Pack)]
struct MyPubkey([u8; 32]);

impl DedupeEncodeable for MyPubkey {}
impl DedupeDecodeable for MyPubkey {}
```

### Incremental diff encoding

`DiffEncoder`/`DiffDecoder` provide stateful delta encoding for keyed byte blobs. When the same key is re‑encoded, only the diff is emitted. Two strategies are tried automatically and the smaller output is picked:

- **RLE patches** — run‑length‑encoded list of changed regions (fast, best for sparse changes)
- **XOR + zstd** — XOR old and new blobs, then zstd‑compress the result (best for scattered changes)

```rust
use lencode::prelude::*;
use lencode::context::{EncoderContext, DecoderContext};
use lencode::diff::{DiffEncoder, DiffDecoder};

let key = 1u64;
let mut enc_ctx = EncoderContext { dedupe: None, diff: Some(DiffEncoder::new()) };
let mut dec_ctx = DecoderContext { dedupe: None, diff: Some(DiffDecoder::new()) };

// First encode (full blob)
let data1: Vec<u8> = vec![0xAA; 2048];
let mut buf = Vec::new();
enc_ctx.diff.as_mut().unwrap().set_key(key);
dec_ctx.diff.as_mut().unwrap().set_key(key);
data1.encode_ext(&mut buf, Some(&mut enc_ctx)).unwrap();

// Second encode (only the diff is written)
let mut data2 = data1.clone();
data2[100] = 0xFF;
buf.clear();
enc_ctx.diff.as_mut().unwrap().set_key(key);
dec_ctx.diff.as_mut().unwrap().set_key(key);
data2.encode_ext(&mut buf, Some(&mut enc_ctx)).unwrap();
assert!(buf.len() < data2.len() / 2); // diff is much smaller

let mut cursor = Cursor::new(&buf[..]);
let result: Vec<u8> = Vec::decode_ext(&mut cursor, Some(&mut dec_ctx)).unwrap();
assert_eq!(result, data2);
```

### Writer pre‑allocation

The `Write` trait provides a `reserve(additional)` hint. Growable writers like `VecWriter` use this to pre‑allocate capacity before encoding large collections, reducing intermediate reallocations.

## Supported types

- Primitives: all ints, `bool`, `f32`, `f64`
- Arrays: `[T; N]`
- Option: `Option<T>`
- Bytes/strings: `&[u8]`, `Vec<u8]`, `VecDeque<u8]`, `&str`, `String`
- Collections (alloc): `Vec<T>`, `BTreeMap<K,V>`, `BTreeSet<V>`, `VecDeque<T>`, `LinkedList<T>`, `BinaryHeap<T>`
- Tuples: `(T1,)` … up to 11 elements
- `std` feature: adds support for `std::borrow::Cow<'_, T>`
- `solana` feature: `Pubkey`, `Signature`, `Hash`, messages (legacy/v0), and related v2/v3 types

Note: `HashMap`/`HashSet` are not implemented.

## Cargo features

- `default`: core + `no_std` (uses `alloc`)
- `std`: enables `std` adapters and `Cow`
- `solana`: Solana SDK v2 + Agave v3 types (implies `std`)

## Big‑endian and portability

- Varints are decoded efficiently on little‑endian and portably on big‑endian
- CI runs tests on `s390x-unknown-linux-gnu` using `cross`
- `Pack` always uses a stable little‑endian layout

## Benchmarks

```bash
# Full suite
cargo bench --all-features

# Compare against borsh/bincode
cargo bench --bench roundup --features std

# Diff encoder (RLE vs XOR+zstd strategies)
cargo bench --bench diff_bench --features std

# Solana‑specific
cargo bench --bench solana_bench --features solana
```

## Errors

Errors use `lencode::io::Error` and map to `std::io::Error` under `std`.

```rust
use lencode::prelude::*;
use lencode::io::Error;

let mut buf = Vec::new();
match encode(&123u64, &mut buf) {
    Ok(n) => eprintln!("wrote {n} bytes"),
    Err(Error::WriterOutOfSpace) => eprintln!("buffer too small"),
    Err(Error::ReaderOutOfData) => eprintln!("unexpected EOF"),
    Err(Error::InvalidData) => eprintln!("corrupted data"),
    Err(e) => eprintln!("other error: {e}"),
}
```

## Examples

- `examples/size_comparison.rs`: space savings on repeated Solana pubkeys
- `examples/versioned_tx_compression.rs`: end‑to‑end on Solana versioned transactions

Run with `--features solana`.

## License

MIT

## Changelog

### 0.1.0
- Lencode varints for signed/unsigned ints
- Optional deduplication via `DedupeEncoder`/`DedupeDecoder`
- Bytes/strings with flagged header + zstd
- `no_std` by default; `std` and `solana` features
- Derive macros for `Encode`/`Decode`
- Big‑endian test coverage
