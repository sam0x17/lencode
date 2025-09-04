# Lencode

[![Crates.io](https://img.shields.io/crates/v/lencode.svg)](https://crates.io/crates/lencode)
[![Documentation](https://docs.rs/lencode/badge.svg)](https://docs.rs/lencode)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A Rust crate for encoding and decoding variable-length data using a high
performance varint encoding scheme with built-in deduplication support.

## Features

- 🚀 **High Performance**: Optimized variable-length integer encoding with minimal overhead
- 🔄 **Deduplication**: Built-in value deduplication to reduce data size for repeated values
- 🌐 **No-std Compatible**: Works in `no_std` environments for embedded and blockchain applications
- 📦 **Comprehensive Types**: Support for primitives, collections, tuples, and custom types
- ⛓️ **Solana Integration**: Optional Solana blockchain types support with efficient pubkey deduplication
- 🎯 **Zero-copy**: Efficient cursor-based I/O operations
- 📊 **Benchmarked**: Performance-tested against popular serialization libraries

## Quick Start

Add lencode to your `Cargo.toml`:

```toml
[dependencies]
lencode = "0.1"

# For Solana support
lencode = { version = "0.1", features = ["solana"] }

# For std support
lencode = { version = "0.1", features = ["std"] }
```

## Basic Usage

### Simple Encoding/Decoding

```rust
use lencode::prelude::*;

// Encode a value
let value = 42u64;
let mut buffer = Vec::new();
let bytes_written = value.encode(&mut buffer)?;

// Decode the value
let mut cursor = Cursor::new(&buffer);
let decoded: u64 = u64::decode(&mut cursor)?;
assert_eq!(value, decoded);
```

### With Deduplication

```rust
use lencode::prelude::*;

// Values with duplicates
let values = vec![100u32, 200u32, 100u32, 300u32, 200u32];

// Encode with deduplication
let mut buffer = Vec::new();
let mut encoder = DedupeEncoder::new();
let bytes_written = values.encode_ext(&mut buffer, Some(&mut encoder))?;

// Decode with deduplication
let mut cursor = Cursor::new(&buffer);
let mut decoder = DedupeDecoder::new();
let decoded_values: Vec<u32> = Vec::decode_ext(&mut cursor, Some(&mut decoder))?;

assert_eq!(values, decoded_values);
// With deduplication, repeated values only store a reference after first occurrence
```

### Solana Pubkey Deduplication

```rust
#[cfg(feature = "solana")]
use solana_sdk::pubkey::Pubkey;
use lencode::prelude::*;

// Pubkeys in Solana transactions often repeat
let pubkey1 = Pubkey::new_unique();
let pubkey2 = Pubkey::new_unique();
let pubkeys = vec![pubkey1, pubkey2, pubkey1, pubkey1, pubkey2]; // Duplicates

// Pubkeys REQUIRE deduplication - they will error without it
let mut buffer = Vec::new();
let mut encoder = DedupeEncoder::new();
let bytes_written = pubkeys.encode_ext(&mut buffer, Some(&mut encoder))?;

// First occurrence: 33 bytes (1 + 32), subsequent: 1 byte each
// Total: 69 bytes vs 160 bytes without deduplication (56% savings!)

// Decode with deduplication
let mut cursor = Cursor::new(&buffer);
let mut decoder = DedupeDecoder::new();
let decoded_pubkeys: Vec<Pubkey> = Vec::decode_ext(&mut cursor, Some(&mut decoder))?;

assert_eq!(pubkeys, decoded_pubkeys);
```

## Core Concepts

### Variable-Length Encoding

Lencode uses an optimized lencode varint encoding scheme that minimizes space usage:

- Small integers use fewer bytes
- Large integers use more bytes as needed
- Signed integers use zigzag encoding for efficient negative number representation

### Deduplication System

The deduplication system tracks previously encoded values:

- **First occurrence**: Encoded with ID 0 + full value data
- **Subsequent occurrences**: Only the reference ID (variable-length integer)
- **Memory efficient**: Uses HashMap for O(1) lookups
- **Configurable**: Optional - pass `None` to disable

### Traits

- **`Encode`**: Convert values to bytes with optional deduplication
- **`Decode`**: Convert bytes back to values with optional deduplication  
- **`Pack`**: Platform-independent byte representation (used internally by deduplication)

## Supported Types

### Primitives
- All integer types: `u8`, `i8`, `u16`, `i16`, `u32`, `i32`, `u64`, `i64`, `u128`, `i128`, `usize`, `isize`
- Floating-point: `f32`, `f64`
- Boolean: `bool`
- Arrays: `[T; N]` where `T: Encode + Decode`

### Collections
- `Vec<T>`
- `HashMap<K, V>`, `BTreeMap<K, V>`
- `HashSet<T>`, `BTreeSet<T>`
- `VecDeque<T>`, `LinkedList<T>`, `BinaryHeap<T>`
- `Option<T>`
- `String`

### Tuples
- Up to 11-element tuples: `(T1,)`, `(T1, T2)`, ..., `(T1, T2, ..., T11)`

### Blockchain Types (with `solana` feature)
- `Pubkey` - with optimized deduplication
- `Signature`
- `Hash`
- `MessageHeader`

### Custom Types
- Implement `Encode`, `Decode`, and `Pack` traits for your own types

## Performance

Lencode is designed for high performance and has been benchmarked against popular serialization libraries:

```bash
# Run benchmarks
cargo bench --all-features

# Compare with borsh and bincode
cargo bench --bench roundup

# Solana-specific benchmarks  
cargo bench --bench solana_bench --features solana
```

Typical performance characteristics:
- **Encoding**: Competitive with bincode, faster than borsh for variable-length data
- **Decoding**: Optimized cursor-based reading with minimal allocations
- **Deduplication**: Significant space savings (30-70%) for data with repeated values
- **Memory**: Low memory overhead, configurable buffer sizes

## Advanced Usage

### Custom Types

```rust
use lencode::prelude::*;

#[derive(Debug, PartialEq)]
struct Point {
    x: f64,
    y: f64,
}

impl Encode for Point {
    fn encode_ext(&self, writer: &mut impl Write, dedupe_encoder: Option<&mut DedupeEncoder>) -> Result<usize> {
        let mut bytes = 0;
        bytes += self.x.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        bytes += self.y.encode_ext(writer, dedupe_encoder)?;
        Ok(bytes)
    }
}

impl Decode for Point {
    fn decode_ext(reader: &mut impl Read, dedupe_decoder: Option<&mut DedupeDecoder>) -> Result<Self> {
        let x = f64::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let y = f64::decode_ext(reader, dedupe_decoder)?;
        Ok(Point { x, y })
    }
}

impl Pack for Point {
    fn pack(&self, writer: &mut impl Write) -> Result<usize> {
        let mut bytes = 0;
        bytes += self.x.pack(writer)?;
        bytes += self.y.pack(writer)?;
        Ok(bytes)
    }
    
    fn unpack(reader: &mut impl Read) -> Result<Self> {
        let x = f64::unpack(reader)?;
        let y = f64::unpack(reader)?;
        Ok(Point { x, y })
    }
}
```

### Low-level Varint Operations

```rust
use lencode::prelude::*;

// Direct varint encoding/decoding
let value = 12345u64;
let mut buffer = Vec::new();
Lencode::encode_varint(value, &mut buffer)?;

let mut cursor = Cursor::new(&buffer);
let decoded = Lencode::decode_varint::<u64>(&mut cursor)?;
```

## Features

- **`default`**: Core functionality only
- **`std`**: Standard library support (collections, etc.)
- **`solana`**: Solana blockchain types support (implies `std`)

## Error Handling

Lencode provides comprehensive error handling:

```rust
use lencode::io::Error;

match value.encode(&mut buffer) {
    Ok(bytes_written) => println!("Encoded {} bytes", bytes_written),
    Err(Error::WriterOutOfSpace) => println!("Buffer too small"),
    Err(Error::ReaderOutOfData) => println!("Unexpected end of data"),
    Err(Error::InvalidData) => println!("Corrupted data"),
    Err(e) => println!("Other error: {:?}", e),
}
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request. For major changes, please open an issue first to discuss what you would like to change.

### Development

```bash
# Run tests
cargo test --all-features

# Run benchmarks
cargo bench --all-features

# Check no-std compatibility
cargo check --no-default-features

# Format code
cargo fmt

# Run clippy
cargo clippy --all-features
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Changelog

### 0.1.0 (Initial Release)
- Variable-length integer encoding with lencode varint scheme
- Deduplication system for space optimization
- Support for primitives, collections, and tuples
- Solana blockchain types integration
- No-std compatibility
- Comprehensive test suite and benchmarks
