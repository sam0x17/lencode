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
- Bytes/strings compression: flagged header + zstd when smaller
- no_std + alloc: works without `std` (uses `zstd-safe`)
- Derive macros: `#[derive(Encode, Decode)]` for your types
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

The encoder picks whichever is smaller per value.

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
