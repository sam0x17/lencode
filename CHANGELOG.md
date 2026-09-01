# Changelog

This file records user-visible changes to lencode. Versions follow Semantic
Versioning.

## [2.0.0] - 2026-09-01

### Breaking changes

- The `solana` feature now targets the current Solana v4 and Agave 4.x beta
  crates instead of the v3 type family. Downstream users must upgrade their
  Solana dependencies or use `solana-primitives` for the stable address, hash,
  and signature implementations.
- The minimum supported Rust version is now 1.98.0.
- `Error` adds `DecodeLimitExceeded` and `TrailingData`. Exhaustive matches on
  this enum must handle the new variants.
- Native lencode varint decoders now reject oversized and non-canonical
  encodings. Bytes emitted by 1.x encoders remain valid.
- The byte and string compression policy changed. Version 2 may choose a
  different raw or Zstd representation while remaining decodable by readers
  that support the existing flagged byte format.

### Added

- `decode_exact`, `decode_exact_ext`, and `decode_exact_with_limits` reject
  trailing bytes and bound input, collection sizes, and allocation claims.
- `DiffPolicy` adds explicit adaptive, RLE-only, outer-compressed, and disabled
  strategies. Outer-compressed mode emits raw XOR frames for a containing
  compressor. Diff encoder and decoder caches can be bounded by retained bytes
  and key count.
- Diff-frame skipping APIs validate framing without materializing account data.
- `DedupeIdCodec` allows a versioned outer format to select canonical unsigned
  LEB128 IDs. Existing constructors continue to use lencode's native integer
  codec.
- The `solana-primitives` feature implements lencode traits for the official
  lightweight Solana address, hash, and signature crates.
- The `solana-types` feature adds current reference transaction types,
  canonical transaction reconstruction, and versioned canonical or semantic
  LZ4 entry-batch codecs.
- Borrowed and caller-owned buffer APIs reduce copies in dedupe, diff, and
  Solana entry-batch decode paths.

### Security and correctness

- Zstd byte and diff payloads must contain exactly one frame. Concatenated or
  trailing frames are rejected.
- Streaming readers handle short reads and reject zero-progress or
  over-reported reads without exposing uninitialized memory.
- Collection-size multiplication, `usize` conversion, LZ4 expansion, cache
  growth, and decoded blob size are checked before allocation or mutation.
- Failed diff and Solana frame decodes leave reusable state and caller output
  unchanged.

### Wire compatibility

- Canonical native integer encodings and existing diff modes 0 through 2 are
  unchanged.
- `DiffEncoder::new()` retains the adaptive modes 0 through 2 behavior. Raw XOR
  mode 3 is emitted only when `DiffPolicy::OuterCompressed` is selected.
  `DiffDecoder::new()` and `skip_diff_blob_frame()` also remain limited to
  modes 0 through 2. A containing format must version mode 3 and opt into it
  explicitly.
- Alternate dedupe ID codecs and the `LCSH` Solana entry-batch envelope are
  explicitly selected. They are not silent changes to existing frames.

[2.0.0]: https://github.com/sam0x17/lencode/compare/v1.2.1...v2.0.0
