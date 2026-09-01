# lencode

[![Crates.io](https://img.shields.io/crates/v/lencode.svg)](https://crates.io/crates/lencode)
[![docs.rs](https://docs.rs/lencode/badge.svg)](https://docs.rs/lencode)
[![CI](https://github.com/sam0x17/lencode/actions/workflows/ci.yaml/badge.svg?branch=main)](https://github.com/sam0x17/lencode/actions/workflows/ci.yaml)
[![Big Endian CI](https://github.com/sam0x17/lencode/actions/workflows/big-endian.yml/badge.svg?branch=main)](https://github.com/sam0x17/lencode/actions/workflows/big-endian.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Compact binary encoding with varints, optional deduplication, and opportunistic
Zstd compression for bytes and strings. `no_std` by default, with an opt-in
`std` feature.

## Highlights

- Fast varints: efficient for small and large integers
- Optional deduplication: replace repeats with compact IDs for supported types
- Exact, bounded decoding: reject trailing bytes and cap input, sequence, blob,
  and allocation sizes
- Bytes/strings compression: flagged header + Zstd when smaller; high-entropy
  data is detected and skipped automatically
- Bulk encoding: `Vec<T>` of fixed-size types such as `[u8; 32]` use bulk
  `memcpy` instead of per-element encode and decode calls
- no_std + alloc: works without `std` (uses `zstd-safe`)
- Derive macros: `#[derive(Encode, Decode)]` for your types, `#[derive(Pack)]` for dedupe/bulk types
- Solana support: feature `solana-primitives` adds official lightweight
  address, hash, and signature types; `solana-types` adds current reference
  messages and transactions; `solana` adds the broader Agave adapters
- Agave transaction bridge: feature `std` includes a bounded, reusable compact-to-canonical
  transcoder for current legacy, v0, and v1 transaction wire layouts
- Native entry-batch bridge: feature `solana-types` encodes reference transactions, supports
  dictionary-backed semantic LZ4, and validates the versioned `LCSH` envelope inside lencode
- Big-endian ready: CI runs tests on s390x

## Install

```toml
[dependencies]
lencode = "2"

# With standard library types (e.g., Cow)
lencode = { version = "2", features = ["std"] }

# With stable, lightweight Solana primitives (implies std)
lencode = { version = "2", features = ["solana-primitives"] }

# With lightweight Solana message/transaction support (implies std)
lencode = { version = "2", features = ["solana-types"] }

# With the broader Agave runtime/status/Geyser adapters too
lencode = { version = "2", features = ["solana"] }
```

## Upgrading from 1.x

Version 2 moves the `solana` feature from the Solana v3 type family to current
Solana v4 and stable Agave 4.2 crates, and raises the minimum Rust version to
1.98.0. Canonical 1.x lencode integers and diff modes remain decodable, but
decoders now reject non-canonical inputs. See the [changelog](CHANGELOG.md) for
the complete compatibility notes.

## Quick start

### Derive and round-trip

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

To benefit from deduplication for your own types, implement `Pack` and the
marker traits and pass the encoder/decoder via `encode_ext`/`decode_ext`.

```rust
use lencode::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
struct MyId(u32);

impl Pack for MyId {
    fn pack(&self, w: &mut impl Write) -> Result<usize> { self.0.pack(w) }
    fn unpack(r: &mut impl Read) -> Result<Self> { Ok(Self(u32::unpack(r)?)) }
}
impl DedupeEncodeable for MyId {
    type Hasher = DefaultDedupeHasher;
}
impl DedupeDecodeable for MyId {
    type Hasher = DefaultDedupeHasher;
}

let vals = vec![MyId(42), MyId(7), MyId(42), MyId(7), MyId(42)];

// Encode with deduplication enabled
let mut enc_ctx = EncoderContext::with_dedupe();
let mut buf = Vec::new();
encode_ext(&vals, &mut buf, Some(&mut enc_ctx))?;

// Decode with deduplication enabled
let mut dec_ctx = DecoderContext::with_dedupe();
let roundtrip: Vec<MyId> = decode_ext(&mut Cursor::new(&buf), Some(&mut dec_ctx))?;
assert_eq!(roundtrip, vals);
```

### Compact bytes and strings

`&[u8]`, `Vec<u8>`, `VecDeque<u8>`, `&str`, and `String` use a compact flagged
header: `varint((payload_len << 1) | flag) + payload`.

- `flag = 0`: raw bytes/UTF-8
- `flag = 1`: one Zstd frame (original size stored inside the frame)

The encoder picks whichever is smaller per value. High-entropy data such as
random or encrypted bytes skips compression after a fast entropy check.

### Bulk encoding for fixed-size types

`Vec<T>` where `T` has a fixed-size wire representation, such as `[u8; 32]`
or a `#[repr(transparent)]` newtype over a byte array, uses bulk `memcpy`. This
is handled through `Encode::encode_slice` / `Decode::decode_vec` and their
`Pack` counterparts `Pack::pack_slice` / `Pack::unpack_vec`.

Custom `Pack` types can opt in by overriding `pack_slice` and `unpack_vec`, or
by using `#[derive(Pack)]` on a `#[repr(transparent)]` single-field struct. The
derive generates the bulk overrides automatically:

```rust
use lencode::prelude::*;

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Pack)]
struct MyPubkey([u8; 32]);

impl DedupeEncodeable for MyPubkey {
    type Hasher = DefaultDedupeHasher;
}
impl DedupeDecodeable for MyPubkey {
    type Hasher = DefaultDedupeHasher;
}
```

Existing dedupe contexts encode IDs with lencode's native integer format.
Versioned formats can instead pair `DedupeEncoder::with_frozen_codec` and
`DedupeDecoder::with_frozen_codec` with `DedupeIdCodec::UnsignedLeb128`.
Encoder and decoder must use the same ordered frozen dictionary and ID codec.

### Incremental diff encoding

`DiffEncoder`/`DiffDecoder` provide stateful delta encoding for keyed byte
blobs. When the same key is re-encoded, the default adaptive policy tries two
strategies and picks the smaller output:

- **RLE patches**: run-length-encoded changed regions, intended for sparse changes
- **XOR + Zstd**: XOR old and new blobs, then compress the result, intended for scattered changes

`DiffPolicy::OuterCompressed` instead compares RLE with raw XOR so a larger
containing Zstd frame can compress across records. This policy can emit wire
mode 3. Select it only in a versioned outer format and construct its decoder
with `DiffDecoder::with_max_supported_mode(RAW_XOR_DIFF_MODE)`.
`DiffEncoder::new()`, `DiffDecoder::new()`, and the default skip helper remain
limited to modes 0 through 2.

Supported byte types: `Vec<u8>`, `&[u8]`, `[u8; N]`, `VecDeque<u8>`. Set a key
via `set_key()` on the diff encoder/decoder before each
`encode_ext`/`decode_ext` call to opt in.

On decode, `DiffDecoder::decode_blob` returns an owned `Vec<u8>`, while
`DiffDecoder::decode_blob_ref` borrows the reconstruction from the decoder's
store. The borrowed path is allocation-free in steady state because buffers
are recycled across calls. Prefer it when the caller immediately copies the
result into its own storage.

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

### Writer pre-allocation

The `Write` trait provides a `reserve(additional)` hint. Growable writers such
as `VecWriter` use this to pre-allocate capacity before encoding large
collections, reducing intermediate reallocations.

### Exact and resource-bounded decoding

Use `decode_exact` when one byte slice must contain exactly one value. For
untrusted input, `decode_exact_with_limits` also caps encoded bytes, collection
elements, and cumulative allocation. `LimitedReader::with_max_blob_bytes`
allows large account data without also permitting an equally large collection
element count.

### Agave transaction wire reconstruction

`solana_wire::SolanaTransactionTranscoder` reconstructs the canonical
transaction bytes consumed by current Agave byte-backed transaction views. It
is independent of a particular Solana SDK crate version, borrows frozen
address-dictionary hits, reuses decompression scratch space, and can append
directly to a reusable block-component buffer. Every transaction is
independently length-framed and decoded with explicit input, output, sequence,
and allocation limits.

With `solana-types`, `SolanaEntryBatchEncoder` accepts borrowed reference-Solana
entries and owns the versioned `LCSH` envelope, per-transaction framing,
dictionary identity, and dedupe resets. `SolanaEntryBatchTranscoder` validates
the same envelope and reconstructs one bounded canonical entry batch. Its
dictionary frame flag is distinct from Agave's earlier canonical-LZ4 prototype,
so both formats remain explicitly versioned without ambiguous decoding.

`SolanaSemanticLz4EntryBatchEncoder` combines contextual LEB128 pubkey IDs with
LZ4 compression. It keeps the compact batch as arena scratch and returns the
compressed frame by borrow, avoiding a final frame copy. Its selective entry
point skips one-FEC batches and returns a semantic frame only when it removes a
complete wire block. `SolanaPubkeyDictionary` constructs the encoder and
decoder states together from one ordered artifact, rejects duplicates, and
derives the 16-byte wire ID from a domain-separated SHA-256 digest. Pubkey
lookups use a keyed full-value hash because shred payload keys are untrusted.

`SolanaLz4EntryBatchDecoder` dispatches canonical and semantic frames without
fallback, applies separate compressed, expanded, sequence, and allocation
limits, and reuses its semantic scratch buffer. Unknown versions, flags, and
dictionary IDs fail closed.

This is a bridge into Agave's existing zero-copy parser, not a drop-in Turbine
wire change. Putting the compact representation into shreds changes Merkle
roots and shred signatures, so a network deployment needs an explicit feature
activation, format version, and identical dictionary identity on leaders and
validators. `transcode_exact` and `transcode_append_exact` reject trailing data
and roll back their output on failure. Callers should reject unknown versions
or dictionary IDs instead of trying another codec.

An outer format may explicitly flag a contextual transaction stream to
deduplicate non-frozen addresses across its frames. Clear the encoder once at
the outer boundary. On decode, call `reset_context` at the same boundary and
`transcode_append_exact_continuing` for each frame. This mode is sequential and
must use a distinct outer-format flag. Reset or discard the decoder after any
error. The existing transcoding methods continue to reset per transaction.

## Supported types

- Primitives: all ints, `bool`, `f32`, `f64`
- Arrays: `[T; N]`
- Option: `Option<T>`
- Bytes/strings: `&[u8]`, `Vec<u8>`, `VecDeque<u8>`, `&str`, `String`
- Collections (alloc): `Vec<T>`, `BTreeMap<K,V>`, `BTreeSet<V>`, `VecDeque<T>`,
  `LinkedList<T>`, `BinaryHeap<T>`
- Tuples: `(T1,)` through 11 elements
- `std` feature: adds `std::borrow::Cow<'_, T>`, `HashMap<K, V>`,
  `HashSet<T>`, and the bounded transaction transcoder
- `solana-primitives` feature: official lightweight `Address`, `Hash`, and
  `Signature` implementations
- `solana-types` feature: current reference `Pubkey`, `Signature`, `Hash`, and
  legacy/v0/v1 messages and transactions
- `solana` feature: the lightweight types plus related current Agave runtime,
  status, and Geyser types

## Cargo features

- `default`: core + `no_std` (uses `alloc`)
- `std`: enables `std` adapters, `Cow`, and the Agave transaction wire
  transcoder
- `solana-primitives`: official lightweight Solana address, hash, and
  signature types (implies `std`)
- `solana-types`: lightweight current Solana message and transaction types
  (implies `std`)
- `solana`: broader current Solana and Agave runtime/status/Geyser types (implies `solana-types`)

## Big-endian and portability

- Varints are decoded efficiently on little-endian and portably on big-endian
- CI runs tests on `s390x-unknown-linux-gnu` using `cross`
- `Pack` always uses a stable little-endian layout

## Benchmarks

```bash
# Full suite
cargo bench --all-features

# Compare against borsh/bincode
cargo bench --bench roundup --features std

# Diff encoder (RLE vs XOR+zstd strategies)
cargo bench --bench diff_bench --features std

# Solana-specific
cargo bench --bench solana_bench --features solana-types,comparison-bench
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
- `examples/versioned_tx_compression.rs`: end-to-end on Solana versioned transactions

Run with `--features solana-types`.

## License

MIT
