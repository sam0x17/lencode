//! Bounded reconstruction of canonical Solana transaction bytes.
//!
//! This module is intentionally independent of Solana SDK crate versions. Its
//! lencode side uses `[u8; 32]` for addresses, so an Agave integration can
//! prime one stable address dictionary and keep using it across SDK upgrades.
//! The output matches the canonical wincode transaction layout consumed by
//! Agave's byte-backed transaction views.
//!
//! The compact input layout is:
//!
//! ```text
//! signatures_len(varint) | signatures(raw 64-byte values) | message_version(varint)
//! legacy/v0: header(packed varint) | addresses(deduped) | hash | instructions | [lookups]
//! v1:        header(packed varint) | config mask/values(varints) | hash |
//!            addresses(deduped) | instructions
//! ```
//!
//! Collection counts use lencode varints. Instruction account/data vectors
//! use lencode's flagged raw-or-zstd byte representation. Message versions are
//! `0` for legacy, `1` for v0, and `2` for v1. A caller that stores a stream of
//! transactions should length-frame each compact transaction so this module
//! can enforce exact consumption. The entry-batch envelope supports both
//! independently recoverable transactions and a contextual mode that shares
//! novel address IDs across the batch.
//!
//! With `solana-types`, `SolanaEntryBatchEncoder` accepts current reference
//! `VersionedTransaction` values and owns the complete compact `LCSH` batch
//! envelope. `SolanaEntryBatchTranscoder` validates that envelope and
//! reconstructs the canonical entry-batch bytes expected by Agave.
//! [`SolanaCanonicalLz4EntryBatchEncoder`] provides the lower-latency
//! alternative: it writes the same canonical bytes directly from reference
//! Solana types, then wraps them in the compatible flag-1 LZ4 frame.

use std::{sync::Arc, vec::Vec};

#[cfg(feature = "solana-types")]
use std::mem::{MaybeUninit, size_of};

use crate::{
    Decode, Lencode, Result,
    bytes::{zstd_content_size, zstd_decompress_into},
    dedupe::{DedupeDecoder, FrozenDecoderState},
    io::{Cursor, DecodeLimits, Error, LimitedReader, Read},
};

#[cfg(feature = "solana-types")]
use crate::{
    Encode,
    context::EncoderContext,
    dedupe::{DedupeEncoder, FrozenEncoderState},
    io::{VecWriter, Write},
};
#[cfg(feature = "solana-types")]
use lz4::block::{self as lz4_block, CompressionMode};
#[cfg(feature = "solana-types")]
use solana_hash::Hash as SolanaHash;
#[cfg(feature = "solana-types")]
use solana_message::VersionedMessage;
#[cfg(feature = "solana-types")]
use solana_transaction::versioned::VersionedTransaction;

/// Magic prefix for a compact Solana entry batch.
pub const SOLANA_ENTRY_BATCH_MAGIC: [u8; 4] = *b"LCSH";
/// Compact Solana entry-batch format version.
pub const SOLANA_ENTRY_BATCH_VERSION: u8 = 1;
/// Frame flag for canonical Solana entry batches compressed as an LZ4 block.
pub const SOLANA_ENTRY_BATCH_CANONICAL_LZ4_FLAG: u8 = 1;
/// Frame flag for independently recoverable lencode transactions using an address dictionary.
pub const SOLANA_ENTRY_BATCH_DICTIONARY_FLAG: u8 = 2;
/// Frame flag for native lencode transactions sharing address IDs across the batch.
pub const SOLANA_ENTRY_BATCH_CONTEXTUAL_DICTIONARY_FLAG: u8 = 4;
/// Contextual frame flag using LEB128 for reference-Solana address IDs.
pub const SOLANA_ENTRY_BATCH_CONTEXTUAL_LEB128_DICTIONARY_FLAG: u8 = 8;
/// Bytes used to identify the frozen address dictionary.
pub const SOLANA_DICTIONARY_ID_BYTES: usize = 16;
/// Fixed compact entry-batch header size.
pub const SOLANA_ENTRY_BATCH_HEADER_BYTES: usize =
    SOLANA_ENTRY_BATCH_MAGIC.len() + 1 + 1 + SOLANA_DICTIONARY_ID_BYTES;
/// Fixed header size for the canonical-LZ4 entry-batch variant.
pub const SOLANA_CANONICAL_LZ4_HEADER_BYTES: usize = SOLANA_ENTRY_BATCH_MAGIC.len() + 1 + 1;

#[cfg(feature = "solana-types")]
const STRONG_RECOMPRESSION_WINDOW_DIVISOR: usize = 10;
#[cfg(feature = "solana-types")]
const EXTREME_OVERSIZED_FAST2_MULTIPLIER: usize = 11;
#[cfg(feature = "solana-types")]
const HC_RECOMPRESSION_WINDOW_DIVISOR: usize = 32;
#[cfg(feature = "solana-types")]
const HC_RECOMPRESSION_LEVEL: i32 = 3;
// Use cheaper HC(2) for the closest boundary band. HC(3) retries any miss.
#[cfg(feature = "solana-types")]
const MID_HC_RECOMPRESSION_WINDOW_DIVISOR: usize = 58;
#[cfg(feature = "solana-types")]
const MID_HC_RECOMPRESSION_LEVEL: i32 = 2;
// The LZ4 block format's maximum achievable compression ratio is about 250.
// Keep a little margin while rejecting impossible output sizes before allocation.
#[cfg(feature = "solana-types")]
const MAX_LZ4_BLOCK_COMPRESSION_RATIO: usize = 256;

/// Canonical version byte for a v0 message.
pub const V0_PREFIX: u8 = 0x80;
/// Canonical version byte for a v1 message.
pub const V1_PREFIX: u8 = 0x81;
/// Maximum lencode frame size accepted for one transaction.
pub const MAX_LENCODE_TRANSACTION_BYTES: usize = 8192;
/// Current maximum canonical Solana transaction size.
pub const MAX_CANONICAL_TRANSACTION_BYTES: usize = 4096;

/// Resource limits for one lencode transaction frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransactionWireLimits {
    /// Maximum bytes accepted in the lencode transaction frame.
    pub max_input_bytes: usize,
    /// Maximum bytes emitted in canonical Solana form.
    pub max_output_bytes: usize,
    /// Maximum element count accepted for any one variable-length field.
    pub max_sequence_len: usize,
    /// Maximum cumulative allocation claims made while decoding.
    pub max_total_allocation: usize,
}

impl TransactionWireLimits {
    /// Limits suitable for current 4 KiB Solana transactions.
    pub const CURRENT: Self = Self {
        max_input_bytes: MAX_LENCODE_TRANSACTION_BYTES,
        max_output_bytes: MAX_CANONICAL_TRANSACTION_BYTES,
        max_sequence_len: u16::MAX as usize,
        max_total_allocation: MAX_CANONICAL_TRANSACTION_BYTES * 16,
    };

    /// Creates explicit transaction wire limits.
    pub const fn new(
        max_input_bytes: usize,
        max_output_bytes: usize,
        max_sequence_len: usize,
        max_total_allocation: usize,
    ) -> Self {
        Self {
            max_input_bytes,
            max_output_bytes,
            max_sequence_len,
            max_total_allocation,
        }
    }
}

impl Default for TransactionWireLimits {
    fn default() -> Self {
        Self::CURRENT
    }
}

/// Resource limits for one compact Solana entry batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EntryBatchWireLimits {
    /// Maximum compact frame size accepted from the network.
    pub max_input_bytes: usize,
    /// Maximum canonical entry-batch bytes reconstructed into the output.
    pub max_output_bytes: usize,
    /// Maximum entry or transaction count accepted in one collection.
    pub max_sequence_len: usize,
    /// Maximum cumulative allocation claims made by the outer decoder.
    pub max_total_allocation: usize,
    /// Limits applied independently to each compact transaction frame.
    pub transaction: TransactionWireLimits,
}

impl EntryBatchWireLimits {
    /// Creates explicit entry-batch wire limits.
    pub const fn new(
        max_input_bytes: usize,
        max_output_bytes: usize,
        max_sequence_len: usize,
        max_total_allocation: usize,
        transaction: TransactionWireLimits,
    ) -> Self {
        Self {
            max_input_bytes,
            max_output_bytes,
            max_sequence_len,
            max_total_allocation,
            transaction,
        }
    }
}

/// Counts recovered while reconstructing one compact entry batch.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EntryBatchCounts {
    /// Number of entries in the batch.
    pub entries: usize,
    /// Total transactions across all entries.
    pub transactions: usize,
}

/// Borrowed reference-Solana entry passed to [`SolanaEntryBatchEncoder`].
#[cfg(feature = "solana-types")]
#[derive(Clone, Copy, Debug)]
pub struct SolanaEntryRef<'a> {
    /// PoH hashes represented by this entry.
    pub num_hashes: u64,
    /// Reference Solana entry hash.
    pub hash: &'a SolanaHash,
    /// Reference Solana transactions in the entry.
    pub transactions: &'a [VersionedTransaction],
}

/// Returns the canonical wincode size of one reference-Solana entry.
///
/// This validates the same collection widths as the canonical encoder and
/// uses checked arithmetic throughout.
#[cfg(feature = "solana-types")]
#[inline]
pub fn canonical_solana_entry_serialized_size(entry: SolanaEntryRef<'_>) -> Result<usize> {
    u64::try_from(entry.transactions.len()).map_err(|_| Error::IncorrectLength)?;
    let mut size = size_of::<u64>()
        .checked_add(entry.hash.as_bytes().len())
        .and_then(|size| size.checked_add(size_of::<u64>()))
        .ok_or(Error::IncorrectLength)?;
    for transaction in entry.transactions {
        checked_add_canonical_size(
            &mut size,
            canonical_solana_transaction_serialized_size_inner(transaction)
                .map(core::num::NonZeroUsize::get)
                .ok_or_else(|| canonical_solana_transaction_size_error(transaction))?,
        )?;
    }
    Ok(size)
}

/// Returns the canonical wincode size of a reference-Solana entry batch.
///
/// The returned size includes the outer entry-count prefix.
#[cfg(feature = "solana-types")]
#[inline]
pub fn canonical_solana_entry_batch_serialized_size<'a>(
    entries: impl ExactSizeIterator<Item = SolanaEntryRef<'a>>,
) -> Result<usize> {
    u64::try_from(entries.len()).map_err(|_| Error::IncorrectLength)?;
    let mut size = size_of::<u64>();
    for entry in entries {
        checked_add_canonical_size(&mut size, canonical_solana_entry_serialized_size(entry)?)?;
    }
    Ok(size)
}

/// Limits and compression policy for canonical-LZ4 Solana entry batches.
#[cfg(feature = "solana-types")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SolanaCanonicalLz4Config {
    /// Maximum uncompressed canonical entry-batch size.
    pub max_canonical_bytes: usize,
    /// Largest canonical input eligible for FAST(4) boundary retries.
    ///
    /// Inputs through eleven times this threshold still use FAST(4) without
    /// retries. Larger inputs use FAST(2). This threshold does not affect
    /// decoder compatibility.
    pub fast_acceleration_max_input: usize,
}

/// Wire-block capacities used to select stronger canonical-LZ4 compression.
#[cfg(feature = "solana-types")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SolanaCanonicalLz4WireBlocks {
    /// Bytes carried by each regular block preceding the final block.
    pub regular_bytes: usize,
    /// Bytes carried when only the final block remains.
    pub final_bytes: usize,
}

#[cfg(feature = "solana-types")]
impl SolanaCanonicalLz4WireBlocks {
    /// Creates an explicit wire-block layout.
    pub const fn new(regular_bytes: usize, final_bytes: usize) -> Self {
        Self {
            regular_bytes,
            final_bytes,
        }
    }
}

#[cfg(feature = "solana-types")]
impl SolanaCanonicalLz4Config {
    /// Creates an explicit canonical-LZ4 policy.
    pub const fn new(max_canonical_bytes: usize, fast_acceleration_max_input: usize) -> Self {
        Self {
            max_canonical_bytes,
            fast_acceleration_max_input,
        }
    }
}

/// Directly encodes reference-Solana entries into a canonical LZ4 shred frame.
///
/// The uncompressed body is byte-for-byte compatible with Agave's canonical
/// wincode entry-batch representation. The outer `LCSH` version-1, flag-1
/// frame is distinct from the dictionary-backed lencode variants and remains
/// decodable regardless of the selected LZ4 acceleration.
#[cfg(feature = "solana-types")]
pub struct SolanaCanonicalLz4EntryBatchEncoder {
    config: SolanaCanonicalLz4Config,
}

#[cfg(feature = "solana-types")]
impl SolanaCanonicalLz4EntryBatchEncoder {
    /// Creates an encoder with explicit output and acceleration limits.
    pub const fn new(config: SolanaCanonicalLz4Config) -> Self {
        Self { config }
    }

    /// Encodes one non-empty entry batch, retaining `output`'s allocation.
    pub fn encode<'a>(
        &mut self,
        entries: impl ExactSizeIterator<Item = SolanaEntryRef<'a>>,
        output: &mut Vec<u8>,
    ) -> Result<EntryBatchCounts> {
        self.encode_with_wire_blocks(entries, output, None)
    }

    /// Encodes one batch and selectively spends more compression work when it
    /// removes a complete wire block.
    ///
    /// Near a boundary, FAST(4) may retry with FAST(1). When the final wire block
    /// is smaller, a narrower boundary window may retry with HC(2) or HC(3).
    /// Every result uses the same standard LZ4 block format and the smallest
    /// mode is selected only at wire-block granularity.
    pub fn encode_for_wire_blocks<'a>(
        &mut self,
        entries: impl ExactSizeIterator<Item = SolanaEntryRef<'a>>,
        output: &mut Vec<u8>,
        wire_blocks: SolanaCanonicalLz4WireBlocks,
    ) -> Result<EntryBatchCounts> {
        self.encode_with_wire_blocks(entries, output, Some(wire_blocks))
    }

    /// Encodes one batch and returns the selected frame borrowed from `output`.
    ///
    /// Bytes before the returned slice are encoder scratch. Passing the slice
    /// directly to the wire consumer avoids compacting the selected LZ4 frame
    /// to the front of the retained arena.
    pub fn encode_for_wire_blocks_in_arena<'entries, 'output>(
        &mut self,
        entries: impl ExactSizeIterator<Item = SolanaEntryRef<'entries>>,
        output: &'output mut Vec<u8>,
        wire_blocks: SolanaCanonicalLz4WireBlocks,
    ) -> Result<(EntryBatchCounts, &'output [u8])> {
        output.clear();
        let result = self.encode_inner(entries, output, Some(wire_blocks));
        if result.is_err() {
            output.clear();
        }
        let (counts, _, frame) = result?;
        Ok((counts, &output[frame]))
    }

    /// Encodes one batch and borrows raw canonical bytes when compression does
    /// not remove a wire block.
    ///
    /// `raw_canonical_bytes` limits fallback to canonical lengths suited to the
    /// ownership or copy policy used by the consumer. The range is half-open.
    /// Both returned forms use existing wire representations accepted by the
    /// canonical-LZ4 feature: raw canonical wincode bytes or an `LCSH` frame.
    pub fn encode_for_wire_blocks_or_raw_in_arena<'entries, 'output>(
        &mut self,
        entries: impl ExactSizeIterator<Item = SolanaEntryRef<'entries>>,
        output: &'output mut Vec<u8>,
        wire_blocks: SolanaCanonicalLz4WireBlocks,
        raw_canonical_bytes: core::ops::Range<usize>,
    ) -> Result<(EntryBatchCounts, &'output [u8])> {
        output.clear();
        let result = self.encode_inner(entries, output, Some(wire_blocks));
        if result.is_err() {
            output.clear();
        }
        let (counts, canonical_len, frame) = result?;
        let use_raw = raw_canonical_bytes.contains(&canonical_len)
            && matches!(
                (
                    wire_block_count(canonical_len, wire_blocks),
                    wire_block_count(frame.len(), wire_blocks),
                ),
                (Some(raw_blocks), Some(frame_blocks)) if raw_blocks == frame_blocks
            );
        let selected = if use_raw { 0..canonical_len } else { frame };
        Ok((counts, &output[selected]))
    }

    fn encode_with_wire_blocks<'a>(
        &mut self,
        entries: impl ExactSizeIterator<Item = SolanaEntryRef<'a>>,
        output: &mut Vec<u8>,
        wire_blocks: Option<SolanaCanonicalLz4WireBlocks>,
    ) -> Result<EntryBatchCounts> {
        output.clear();
        let result = self.encode_inner(entries, output, wire_blocks);
        if result.is_err() {
            output.clear();
        }
        let (counts, _, frame) = result?;
        let frame_len = frame.len();
        output.copy_within(frame, 0);
        output.truncate(frame_len);
        Ok(counts)
    }

    fn encode_inner<'a>(
        &self,
        entries: impl ExactSizeIterator<Item = SolanaEntryRef<'a>>,
        output: &mut Vec<u8>,
        wire_blocks: Option<SolanaCanonicalLz4WireBlocks>,
    ) -> Result<(EntryBatchCounts, usize, core::ops::Range<usize>)> {
        let counts =
            encode_canonical_entry_batch(entries, output, self.config.max_canonical_bytes)?;
        let canonical_len = output.len();
        let compressed_capacity = lz4_block::compress_bound(canonical_len)
            .map_err(|_| Error::IncorrectLength)?
            .checked_add(4)
            .ok_or(Error::IncorrectLength)?;
        let arena_len = canonical_len
            .checked_add(SOLANA_CANONICAL_LZ4_HEADER_BYTES)
            .and_then(|len| len.checked_add(compressed_capacity))
            .ok_or(Error::IncorrectLength)?;
        let acceleration = if canonical_len
            <= self
                .config
                .fast_acceleration_max_input
                .saturating_mul(EXTREME_OVERSIZED_FAST2_MULTIPLIER)
        {
            4
        } else {
            2
        };
        output
            .try_reserve(arena_len - canonical_len)
            .map_err(|_| Error::DecodeLimitExceeded)?;
        output.resize(arena_len, 0);
        let mut frame_offset = canonical_len;
        let mut frame_len = {
            let (canonical, frame) = output.split_at_mut(canonical_len);
            frame[..4].copy_from_slice(&SOLANA_ENTRY_BATCH_MAGIC);
            frame[4] = SOLANA_ENTRY_BATCH_VERSION;
            frame[5] = SOLANA_ENTRY_BATCH_CANONICAL_LZ4_FLAG;
            SOLANA_CANONICAL_LZ4_HEADER_BYTES
                + lz4_block::compress_to_buffer(
                    canonical,
                    Some(CompressionMode::FAST(acceleration)),
                    true,
                    &mut frame[SOLANA_CANONICAL_LZ4_HEADER_BYTES..],
                )
                .map_err(|_| Error::InvalidData)?
        };
        if canonical_len <= self.config.fast_acceleration_max_input
            && wire_blocks.is_some_and(|blocks| should_recompress(frame_len, blocks))
        {
            let strong_offset = arena_len;
            let frame_capacity = arena_len - canonical_len;
            let strong_arena_len = strong_offset
                .checked_add(frame_capacity)
                .ok_or(Error::IncorrectLength)?;
            output
                .try_reserve(frame_capacity)
                .map_err(|_| Error::DecodeLimitExceeded)?;
            output.resize(strong_arena_len, 0);
            let strong_len = {
                let (canonical, tail) = output.split_at_mut(canonical_len);
                let strong_start = strong_offset - canonical_len;
                let strong = &mut tail[strong_start..strong_start + frame_capacity];
                strong[..4].copy_from_slice(&SOLANA_ENTRY_BATCH_MAGIC);
                strong[4] = SOLANA_ENTRY_BATCH_VERSION;
                strong[5] = SOLANA_ENTRY_BATCH_CANONICAL_LZ4_FLAG;
                SOLANA_CANONICAL_LZ4_HEADER_BYTES
                    + lz4_block::compress_to_buffer(
                        canonical,
                        Some(CompressionMode::FAST(1)),
                        true,
                        &mut strong[SOLANA_CANONICAL_LZ4_HEADER_BYTES..],
                    )
                    .map_err(|_| Error::InvalidData)?
            };
            if wire_blocks.is_some_and(|blocks| {
                wire_block_count(strong_len, blocks) < wire_block_count(frame_len, blocks)
            }) {
                frame_offset = strong_offset;
                frame_len = strong_len;
            }
            // Uniform-capacity blocks do not justify the extra HC pass. A
            // smaller final block can still recover a constrained boundary.
            if let Some(blocks) = wire_blocks.filter(|blocks| {
                blocks.regular_bytes != blocks.final_bytes
                    && should_recompress_with_divisor(
                        strong_len,
                        *blocks,
                        HC_RECOMPRESSION_WINDOW_DIVISOR,
                    )
            }) {
                let hc_level = if should_recompress_with_divisor(
                    strong_len,
                    blocks,
                    MID_HC_RECOMPRESSION_WINDOW_DIVISOR,
                ) {
                    MID_HC_RECOMPRESSION_LEVEL
                } else {
                    HC_RECOMPRESSION_LEVEL
                };
                let hc_offset = strong_arena_len;
                let hc_arena_len = hc_offset
                    .checked_add(frame_capacity)
                    .ok_or(Error::IncorrectLength)?;
                output
                    .try_reserve(frame_capacity)
                    .map_err(|_| Error::DecodeLimitExceeded)?;
                output.resize(hc_arena_len, 0);
                let hc_len = {
                    let (canonical, tail) = output.split_at_mut(canonical_len);
                    let hc_start = hc_offset - canonical_len;
                    let hc = &mut tail[hc_start..hc_start + frame_capacity];
                    hc[..4].copy_from_slice(&SOLANA_ENTRY_BATCH_MAGIC);
                    hc[4] = SOLANA_ENTRY_BATCH_VERSION;
                    hc[5] = SOLANA_ENTRY_BATCH_CANONICAL_LZ4_FLAG;
                    let mut hc_len = SOLANA_CANONICAL_LZ4_HEADER_BYTES
                        + lz4_block::compress_to_buffer(
                            canonical,
                            Some(CompressionMode::HIGHCOMPRESSION(hc_level)),
                            true,
                            &mut hc[SOLANA_CANONICAL_LZ4_HEADER_BYTES..],
                        )
                        .map_err(|_| Error::InvalidData)?;
                    if hc_level == MID_HC_RECOMPRESSION_LEVEL
                        && wire_block_count(hc_len, blocks) >= wire_block_count(frame_len, blocks)
                    {
                        hc_len = SOLANA_CANONICAL_LZ4_HEADER_BYTES
                            + lz4_block::compress_to_buffer(
                                canonical,
                                Some(CompressionMode::HIGHCOMPRESSION(HC_RECOMPRESSION_LEVEL)),
                                true,
                                &mut hc[SOLANA_CANONICAL_LZ4_HEADER_BYTES..],
                            )
                            .map_err(|_| Error::InvalidData)?;
                    }
                    hc_len
                };
                if wire_block_count(hc_len, blocks) < wire_block_count(frame_len, blocks) {
                    frame_offset = hc_offset;
                    frame_len = hc_len;
                }
            }
        }
        let frame = frame_offset..frame_offset + frame_len;
        output.truncate(frame.end);
        Ok((counts, canonical_len, frame))
    }
}

#[cfg(feature = "solana-types")]
const fn wire_block_count(len: usize, blocks: SolanaCanonicalLz4WireBlocks) -> Option<usize> {
    if blocks.regular_bytes == 0 || blocks.final_bytes == 0 {
        return None;
    }
    if len <= blocks.final_bytes {
        Some(1)
    } else {
        (len - blocks.final_bytes)
            .div_ceil(blocks.regular_bytes)
            .checked_add(1)
    }
}

#[cfg(feature = "solana-types")]
fn should_recompress(len: usize, blocks: SolanaCanonicalLz4WireBlocks) -> bool {
    // FAST(1) averaged 4.3% smaller than FAST(4) on production-sized
    // components. A 1/10 window covers larger batches while bounding second passes.
    should_recompress_with_divisor(len, blocks, STRONG_RECOMPRESSION_WINDOW_DIVISOR)
}

#[cfg(feature = "solana-types")]
fn should_recompress_with_divisor(
    len: usize,
    blocks: SolanaCanonicalLz4WireBlocks,
    window_divisor: usize,
) -> bool {
    let Some(block_count) = wire_block_count(len, blocks) else {
        return false;
    };
    if block_count <= 1 || window_divisor == 0 {
        return false;
    }
    let Some(previous_capacity) = (block_count - 2)
        .checked_mul(blocks.regular_bytes)
        .and_then(|bytes| bytes.checked_add(blocks.final_bytes))
    else {
        return false;
    };
    len.checked_sub(previous_capacity)
        .is_some_and(|distance| distance <= len / window_divisor)
}

/// Reconstructs one exact canonical-LZ4 entry-batch frame.
///
/// Existing initialized output bytes are reused and overwritten on success.
/// `output` is cleared on every error. The caller still validates the
/// reconstructed canonical entry-batch structure with its normal parser.
#[cfg(feature = "solana-types")]
pub fn transcode_canonical_lz4_entry_batch(
    input: &[u8],
    output: &mut Vec<u8>,
    max_canonical_bytes: usize,
) -> Result<usize> {
    output.clear();
    let result = SolanaCanonicalLz4EntryBatchDecoder::new(input, max_canonical_bytes)
        .and_then(|decoder| decoder.decompress_append_to_vec(output));
    if result.is_err() {
        output.clear();
    }
    result
}

/// A validated canonical-LZ4 entry-batch frame ready to decompress.
///
/// This separates frame validation from output allocation so callers can
/// decode directly into shared or otherwise caller-owned storage.
#[cfg(feature = "solana-types")]
#[derive(Clone, Copy, Debug)]
pub struct SolanaCanonicalLz4EntryBatchDecoder<'a> {
    compressed: &'a [u8],
    compressed_len: i32,
    canonical_len: i32,
}

#[cfg(feature = "solana-types")]
impl<'a> SolanaCanonicalLz4EntryBatchDecoder<'a> {
    /// Validates the frame header and bounded canonical length.
    #[inline]
    pub fn new(input: &'a [u8], max_canonical_bytes: usize) -> Result<Self> {
        if input.len() < SOLANA_CANONICAL_LZ4_HEADER_BYTES + 4
            || input[..4] != SOLANA_ENTRY_BATCH_MAGIC
            || input[4] != SOLANA_ENTRY_BATCH_VERSION
            || input[5] != SOLANA_ENTRY_BATCH_CANONICAL_LZ4_FLAG
        {
            return Err(Error::InvalidData);
        }
        let compressed = &input[SOLANA_CANONICAL_LZ4_HEADER_BYTES..];
        let canonical_len = i32::from_le_bytes(
            compressed[..4]
                .try_into()
                .expect("checked canonical LZ4 length prefix"),
        );
        let canonical_len_usize =
            usize::try_from(canonical_len).map_err(|_| Error::IncorrectLength)?;
        if canonical_len_usize > max_canonical_bytes {
            return Err(Error::DecodeLimitExceeded);
        }
        let compressed = &compressed[4..];
        if canonical_len_usize
            > compressed
                .len()
                .saturating_mul(MAX_LZ4_BLOCK_COMPRESSION_RATIO)
        {
            return Err(Error::InvalidData);
        }
        let compressed_len = i32::try_from(compressed.len()).map_err(|_| Error::IncorrectLength)?;
        Ok(Self {
            compressed,
            compressed_len,
            canonical_len,
        })
    }

    /// Returns the exact output length required by [`Self::decompress_into`].
    pub const fn canonical_len(&self) -> usize {
        self.canonical_len as usize
    }

    /// Decompresses into an initialized slice of exactly the advertised size.
    #[inline]
    pub fn decompress_into(&self, output: &mut [u8]) -> Result<usize> {
        if output.len() != self.canonical_len() {
            return Err(Error::IncorrectLength);
        }
        let written =
            lz4_block::decompress_to_buffer(self.compressed, Some(self.canonical_len), output)
                .map_err(|_| Error::InvalidData)?;
        if written != self.canonical_len() {
            return Err(Error::InvalidData);
        }
        Ok(written)
    }

    /// Appends the decompressed bytes without initializing spare capacity first.
    ///
    /// Existing bytes are retained. On error, the vector length and its existing
    /// bytes are unchanged.
    #[inline]
    pub fn decompress_append_to_vec(&self, output: &mut Vec<u8>) -> Result<usize> {
        let canonical_len = self.canonical_len();
        let new_len = output
            .len()
            .checked_add(canonical_len)
            .ok_or(Error::DecodeLimitExceeded)?;
        output
            .try_reserve(canonical_len)
            .map_err(|_| Error::DecodeLimitExceeded)?;
        self.decompress_into_uninit(&mut output.spare_capacity_mut()[..canonical_len])?;
        // SAFETY: exact decompression success initialized every byte between the
        // old length and `new_len`, which is within the reserved capacity.
        unsafe { output.set_len(new_len) };
        Ok(canonical_len)
    }

    /// Appends into already reserved [`bytes::BytesMut`] spare capacity.
    ///
    /// Returns [`Error::WriterOutOfSpace`] without changing `output` when its
    /// spare capacity is shorter than [`Self::canonical_len`]. Decompression
    /// errors also leave the visible length and existing bytes unchanged.
    #[inline]
    pub fn decompress_append_to_bytes_mut(&self, output: &mut bytes::BytesMut) -> Result<usize> {
        let canonical_len = self.canonical_len();
        let new_len = output
            .len()
            .checked_add(canonical_len)
            .ok_or(Error::DecodeLimitExceeded)?;
        if output.capacity() - output.len() < canonical_len {
            return Err(Error::WriterOutOfSpace);
        }
        self.decompress_into_uninit(&mut output.spare_capacity_mut()[..canonical_len])?;
        // SAFETY: exact decompression success initialized every byte between the
        // old length and `new_len`, and the capacity check bounds `new_len`.
        unsafe { output.set_len(new_len) };
        Ok(canonical_len)
    }

    #[inline]
    fn decompress_into_uninit(&self, output: &mut [MaybeUninit<u8>]) -> Result<usize> {
        if output.len() != self.canonical_len() {
            return Err(Error::IncorrectLength);
        }
        // SAFETY: `new` checked that both lengths fit the signed C parameters.
        // The immutable frame borrow and mutable destination borrow cannot
        // overlap in safe Rust. `output` provides exactly `canonical_len`
        // writable bytes, and LZ4_decompress_safe bounds all reads and writes
        // by the supplied lengths, including for malformed input. Callers expose
        // the destination only after the return value equals `canonical_len`.
        let written = unsafe {
            lz4_sys::LZ4_decompress_safe(
                self.compressed.as_ptr().cast(),
                output.as_mut_ptr().cast(),
                self.compressed_len,
                self.canonical_len,
            )
        };
        if written != self.canonical_len {
            return Err(Error::InvalidData);
        }
        Ok(written as usize)
    }
}

#[cfg(feature = "solana-types")]
// Keep the hot per-transaction return to one word. Every supported transaction
// has nonzero fixed header and hash bytes, so `None` can represent a sizing
// failure. The cold boundary recovers the existing public error variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CanonicalSizeError {
    InvalidData,
    IncorrectLength,
}

#[cfg(feature = "solana-types")]
impl From<Error> for CanonicalSizeError {
    #[cold]
    fn from(error: Error) -> Self {
        match error {
            Error::InvalidData => Self::InvalidData,
            _ => Self::IncorrectLength,
        }
    }
}

#[cfg(feature = "solana-types")]
#[cold]
fn canonical_solana_transaction_size_error(transaction: &VersionedTransaction) -> Error {
    match &transaction.message {
        VersionedMessage::V1(message)
            if usize::from(message.header.num_required_signatures)
                != transaction.signatures.len() =>
        {
            Error::InvalidData
        }
        _ => Error::IncorrectLength,
    }
}

#[cfg(all(feature = "solana-types", test))]
fn canonical_solana_transaction_serialized_size(
    transaction: &VersionedTransaction,
) -> Result<usize> {
    canonical_solana_transaction_serialized_size_inner(transaction)
        .map(core::num::NonZeroUsize::get)
        .ok_or_else(|| canonical_solana_transaction_size_error(transaction))
}

#[cfg(feature = "solana-types")]
fn canonical_solana_transaction_serialized_size_inner(
    transaction: &VersionedTransaction,
) -> Option<core::num::NonZeroUsize> {
    canonical_solana_transaction_serialized_size_fallible(transaction)
        .ok()
        .and_then(core::num::NonZeroUsize::new)
}

#[cfg(feature = "solana-types")]
#[inline(always)]
fn canonical_solana_transaction_serialized_size_fallible(
    transaction: &VersionedTransaction,
) -> core::result::Result<usize, CanonicalSizeError> {
    match &transaction.message {
        VersionedMessage::Legacy(message) => {
            let signature_size =
                canonical_prefixed_signatures_serialized_size(transaction.signatures.len())?;
            let size = u64::try_from(signature_size).map_err(|_| Error::IncorrectLength)?
                + canonical_legacy_message_serialized_size(
                    &message.account_keys,
                    &message.instructions,
                )?;
            usize::try_from(size).map_err(|_| CanonicalSizeError::IncorrectLength)
        }
        VersionedMessage::V0(message) => {
            let signature_size =
                canonical_prefixed_signatures_serialized_size(transaction.signatures.len())?;
            let lookup_prefix =
                canonical_short_u16_serialized_size(message.address_table_lookups.len())?;
            let mut lookup_payload_size = 0u64;
            for lookup in &message.address_table_lookups {
                let writable_len = lookup.writable_indexes.len();
                let readonly_len = lookup.readonly_indexes.len();
                let lookup_size = if (writable_len | readonly_len) < 0x80 {
                    writable_len
                        .checked_add(readonly_len)
                        .and_then(|size| size.checked_add(solana_pubkey::PUBKEY_BYTES + 2))
                        .ok_or(CanonicalSizeError::IncorrectLength)?
                } else {
                    let writable_size =
                        canonical_short_payload_serialized_size(&lookup.writable_indexes)?;
                    let readonly_size =
                        canonical_short_payload_serialized_size(&lookup.readonly_indexes)?;
                    solana_pubkey::PUBKEY_BYTES
                        .checked_add(writable_size)
                        .and_then(|size| size.checked_add(readonly_size))
                        .ok_or(Error::IncorrectLength)?
                };
                let lookup_size =
                    u32::try_from(lookup_size).map_err(|_| CanonicalSizeError::IncorrectLength)?;
                // The validated u16 lookup count and u32 per-lookup size make
                // overflow of this u64 subtotal impossible.
                lookup_payload_size += u64::from(lookup_size);
            }
            let size = u64::try_from(signature_size)
                .and_then(|size| u64::try_from(lookup_prefix).map(|prefix| size + prefix))
                .map_err(|_| CanonicalSizeError::IncorrectLength)?
                + 1
                + canonical_legacy_message_serialized_size(
                    &message.account_keys,
                    &message.instructions,
                )?
                + lookup_payload_size;
            usize::try_from(size).map_err(|_| CanonicalSizeError::IncorrectLength)
        }
        VersionedMessage::V1(message) => {
            let signature_bytes = transaction
                .signatures
                .len()
                .checked_mul(solana_signature::SIGNATURE_BYTES)
                .ok_or(CanonicalSizeError::IncorrectLength)?;
            if usize::from(message.header.num_required_signatures) != transaction.signatures.len() {
                return Err(CanonicalSizeError::InvalidData);
            }
            u8::try_from(message.instructions.len())
                .map_err(|_| CanonicalSizeError::IncorrectLength)?;
            u8::try_from(message.account_keys.len())
                .map_err(|_| CanonicalSizeError::IncorrectLength)?;

            let mut size = size_of::<u8>()
                .checked_add(3)
                .and_then(|size| size.checked_add(size_of::<u32>()))
                .and_then(|size| size.checked_add(solana_hash::HASH_BYTES))
                .and_then(|size| size.checked_add(2 * size_of::<u8>()))
                .ok_or(CanonicalSizeError::IncorrectLength)?;
            checked_add_canonical_items(
                &mut size,
                message.account_keys.len(),
                solana_pubkey::PUBKEY_BYTES,
            )?;
            if message.config.priority_fee.is_some() {
                checked_add_canonical_size(&mut size, size_of::<u64>())?;
            }
            if message.config.compute_unit_limit.is_some() {
                checked_add_canonical_size(&mut size, size_of::<u32>())?;
            }
            if message.config.loaded_accounts_data_size_limit.is_some() {
                checked_add_canonical_size(&mut size, size_of::<u32>())?;
            }
            if message.config.heap_size.is_some() {
                checked_add_canonical_size(&mut size, size_of::<u32>())?;
            }
            checked_add_canonical_items(&mut size, message.instructions.len(), 4)?;
            for instruction in &message.instructions {
                u8::try_from(instruction.accounts.len())
                    .map_err(|_| CanonicalSizeError::IncorrectLength)?;
                u16::try_from(instruction.data.len())
                    .map_err(|_| CanonicalSizeError::IncorrectLength)?;
                checked_add_canonical_size(&mut size, instruction.accounts.len())?;
                checked_add_canonical_size(&mut size, instruction.data.len())?;
            }
            checked_add_canonical_size(&mut size, signature_bytes)?;
            Ok(size)
        }
    }
}

#[cfg(feature = "solana-types")]
fn canonical_prefixed_signatures_serialized_size(len: usize) -> Result<usize> {
    if len == 1 {
        return Ok(1 + solana_signature::SIGNATURE_BYTES);
    }
    let signature_bytes = len
        .checked_mul(solana_signature::SIGNATURE_BYTES)
        .ok_or(Error::IncorrectLength)?;
    canonical_short_u16_serialized_size(len)?
        .checked_add(signature_bytes)
        .ok_or(Error::IncorrectLength)
}

#[cfg(feature = "solana-types")]
fn canonical_legacy_message_serialized_size(
    account_keys: &[solana_pubkey::Pubkey],
    instructions: &[solana_message::compiled_instruction::CompiledInstruction],
) -> Result<u64> {
    let account_prefix = canonical_short_u16_serialized_size(account_keys.len())?;
    let account_count = u64::try_from(account_keys.len()).map_err(|_| Error::IncorrectLength)?;
    Ok(
        3 + u64::try_from(account_prefix).map_err(|_| Error::IncorrectLength)?
            + account_count * solana_pubkey::PUBKEY_BYTES as u64
            + solana_hash::HASH_BYTES as u64
            + canonical_instructions_serialized_size(instructions)?,
    )
}

#[cfg(feature = "solana-types")]
fn canonical_instructions_serialized_size(
    instructions: &[solana_message::compiled_instruction::CompiledInstruction],
) -> Result<u64> {
    let size = canonical_short_u16_serialized_size(instructions.len())?;
    let mut payload_size = 0u64;
    for instruction in instructions {
        let accounts_len = instruction.accounts.len();
        let data_len = instruction.data.len();
        let instruction_size = if (accounts_len | data_len) < 0x80 {
            accounts_len
                .checked_add(data_len)
                .and_then(|size| size.checked_add(3))
                .ok_or(Error::IncorrectLength)?
        } else {
            let accounts_size = canonical_short_payload_serialized_size(&instruction.accounts)?;
            let data_size = canonical_short_payload_serialized_size(&instruction.data)?;
            size_of::<u8>()
                .checked_add(accounts_size)
                .and_then(|size| size.checked_add(data_size))
                .ok_or(Error::IncorrectLength)?
        };
        let instruction_size =
            u32::try_from(instruction_size).map_err(|_| Error::IncorrectLength)?;
        // The validated u16 instruction count and u32 per-instruction size
        // make overflow of this u64 subtotal impossible.
        payload_size += u64::from(instruction_size);
    }
    Ok(u64::try_from(size).map_err(|_| Error::IncorrectLength)? + payload_size)
}

#[cfg(feature = "solana-types")]
fn canonical_short_payload_serialized_size(payload: &[u8]) -> Result<usize> {
    canonical_short_u16_serialized_size(payload.len())?
        .checked_add(payload.len())
        .ok_or(Error::IncorrectLength)
}

#[cfg(feature = "solana-types")]
fn canonical_short_u16_serialized_size(len: usize) -> Result<usize> {
    if len < 0x80 {
        return Ok(1);
    }
    if len < 0x4000 {
        return Ok(2);
    }
    u16::try_from(len).map_err(|_| Error::IncorrectLength)?;
    Ok(3)
}

#[cfg(feature = "solana-types")]
fn checked_add_canonical_items(size: &mut usize, count: usize, item_size: usize) -> Result<()> {
    let additional = count.checked_mul(item_size).ok_or(Error::IncorrectLength)?;
    checked_add_canonical_size(size, additional)
}

#[cfg(feature = "solana-types")]
fn checked_add_canonical_size(size: &mut usize, additional: usize) -> Result<()> {
    *size = size.checked_add(additional).ok_or(Error::IncorrectLength)?;
    Ok(())
}

#[cfg(feature = "solana-types")]
fn encode_canonical_entry_batch<'a>(
    entries: impl ExactSizeIterator<Item = SolanaEntryRef<'a>>,
    output: &mut Vec<u8>,
    max_output: usize,
) -> Result<EntryBatchCounts> {
    let entry_count = entries.len();
    if entry_count == 0 {
        return Err(Error::InvalidData);
    }
    append_canonical(
        output,
        &u64::try_from(entry_count)
            .map_err(|_| Error::IncorrectLength)?
            .to_le_bytes(),
        max_output,
    )?;

    let mut transaction_total = 0usize;
    for entry in entries {
        append_canonical(output, &entry.num_hashes.to_le_bytes(), max_output)?;
        append_canonical(output, entry.hash.as_bytes(), max_output)?;
        transaction_total = transaction_total
            .checked_add(entry.transactions.len())
            .ok_or(Error::IncorrectLength)?;
        append_canonical(
            output,
            &u64::try_from(entry.transactions.len())
                .map_err(|_| Error::IncorrectLength)?
                .to_le_bytes(),
            max_output,
        )?;
        for transaction in entry.transactions {
            encode_canonical_transaction(transaction, output, max_output)?;
        }
    }
    Ok(EntryBatchCounts {
        entries: entry_count,
        transactions: transaction_total,
    })
}

#[cfg(feature = "solana-types")]
fn encode_canonical_transaction(
    transaction: &VersionedTransaction,
    output: &mut Vec<u8>,
    max_output: usize,
) -> Result<()> {
    match &transaction.message {
        VersionedMessage::Legacy(message) => {
            append_canonical_short_u16(output, transaction.signatures.len(), max_output)?;
            append_canonical_signatures(output, &transaction.signatures, max_output)?;
            append_canonical_header(output, &message.header, max_output)?;
            append_canonical_addresses(output, &message.account_keys, max_output)?;
            append_canonical(output, message.recent_blockhash.as_bytes(), max_output)?;
            append_canonical_instructions(output, &message.instructions, max_output)?;
        }
        VersionedMessage::V0(message) => {
            append_canonical_short_u16(output, transaction.signatures.len(), max_output)?;
            append_canonical_signatures(output, &transaction.signatures, max_output)?;
            append_canonical(output, &[V0_PREFIX], max_output)?;
            append_canonical_header(output, &message.header, max_output)?;
            append_canonical_addresses(output, &message.account_keys, max_output)?;
            append_canonical(output, message.recent_blockhash.as_bytes(), max_output)?;
            append_canonical_instructions(output, &message.instructions, max_output)?;
            append_canonical_short_u16(output, message.address_table_lookups.len(), max_output)?;
            for lookup in &message.address_table_lookups {
                append_canonical(output, lookup.account_key.as_array(), max_output)?;
                append_canonical_short_payload(output, &lookup.writable_indexes, max_output)?;
                append_canonical_short_payload(output, &lookup.readonly_indexes, max_output)?;
            }
        }
        VersionedMessage::V1(message) => {
            if usize::from(message.header.num_required_signatures) != transaction.signatures.len() {
                return Err(Error::InvalidData);
            }
            append_canonical(output, &[V1_PREFIX], max_output)?;
            append_canonical_header(output, &message.header, max_output)?;
            let config_mask = solana_message::v1::TransactionConfigMask::from(&message.config).0;
            append_canonical(output, &config_mask.to_le_bytes(), max_output)?;
            append_canonical(output, message.lifetime_specifier.as_bytes(), max_output)?;

            let instruction_count =
                u8::try_from(message.instructions.len()).map_err(|_| Error::IncorrectLength)?;
            let address_count =
                u8::try_from(message.account_keys.len()).map_err(|_| Error::IncorrectLength)?;
            append_canonical(output, &[instruction_count, address_count], max_output)?;
            for address in &message.account_keys {
                append_canonical(output, address.as_array(), max_output)?;
            }
            append_canonical_v1_config(output, &message.config, max_output)?;
            for instruction in &message.instructions {
                let accounts_len =
                    u8::try_from(instruction.accounts.len()).map_err(|_| Error::IncorrectLength)?;
                let data_len =
                    u16::try_from(instruction.data.len()).map_err(|_| Error::IncorrectLength)?;
                append_canonical(
                    output,
                    &[
                        instruction.program_id_index,
                        accounts_len,
                        data_len as u8,
                        (data_len >> 8) as u8,
                    ],
                    max_output,
                )?;
            }
            for instruction in &message.instructions {
                append_canonical(output, &instruction.accounts, max_output)?;
                append_canonical(output, &instruction.data, max_output)?;
            }
            append_canonical_signatures(output, &transaction.signatures, max_output)?;
        }
    }
    Ok(())
}

#[cfg(feature = "solana-types")]
fn append_canonical_header(
    output: &mut Vec<u8>,
    header: &solana_message::MessageHeader,
    max_output: usize,
) -> Result<()> {
    append_canonical(
        output,
        &[
            header.num_required_signatures,
            header.num_readonly_signed_accounts,
            header.num_readonly_unsigned_accounts,
        ],
        max_output,
    )
}

#[cfg(feature = "solana-types")]
fn append_canonical_addresses(
    output: &mut Vec<u8>,
    addresses: &[solana_pubkey::Pubkey],
    max_output: usize,
) -> Result<()> {
    append_canonical_short_u16(output, addresses.len(), max_output)?;
    for address in addresses {
        append_canonical(output, address.as_array(), max_output)?;
    }
    Ok(())
}

#[cfg(feature = "solana-types")]
fn append_canonical_signatures(
    output: &mut Vec<u8>,
    signatures: &[solana_signature::Signature],
    max_output: usize,
) -> Result<()> {
    for signature in signatures {
        append_canonical(output, signature.as_array(), max_output)?;
    }
    Ok(())
}

#[cfg(feature = "solana-types")]
fn append_canonical_instructions(
    output: &mut Vec<u8>,
    instructions: &[solana_message::compiled_instruction::CompiledInstruction],
    max_output: usize,
) -> Result<()> {
    append_canonical_short_u16(output, instructions.len(), max_output)?;
    for instruction in instructions {
        append_canonical(output, &[instruction.program_id_index], max_output)?;
        append_canonical_short_payload(output, &instruction.accounts, max_output)?;
        append_canonical_short_payload(output, &instruction.data, max_output)?;
    }
    Ok(())
}

#[cfg(feature = "solana-types")]
fn append_canonical_v1_config(
    output: &mut Vec<u8>,
    config: &solana_message::v1::TransactionConfig,
    max_output: usize,
) -> Result<()> {
    if let Some(value) = config.priority_fee {
        append_canonical(output, &value.to_le_bytes(), max_output)?;
    }
    if let Some(value) = config.compute_unit_limit {
        append_canonical(output, &value.to_le_bytes(), max_output)?;
    }
    if let Some(value) = config.loaded_accounts_data_size_limit {
        append_canonical(output, &value.to_le_bytes(), max_output)?;
    }
    if let Some(value) = config.heap_size {
        append_canonical(output, &value.to_le_bytes(), max_output)?;
    }
    Ok(())
}

#[cfg(feature = "solana-types")]
fn append_canonical_short_payload(
    output: &mut Vec<u8>,
    payload: &[u8],
    max_output: usize,
) -> Result<()> {
    append_canonical_short_u16(output, payload.len(), max_output)?;
    append_canonical(output, payload, max_output)
}

#[cfg(feature = "solana-types")]
fn append_canonical_short_u16(output: &mut Vec<u8>, len: usize, max_output: usize) -> Result<()> {
    let value = u16::try_from(len).map_err(|_| Error::IncorrectLength)?;
    if value < 0x80 {
        append_canonical(output, &[value as u8], max_output)
    } else if value < 0x4000 {
        append_canonical(
            output,
            &[(value as u8 & 0x7f) | 0x80, (value >> 7) as u8],
            max_output,
        )
    } else {
        append_canonical(
            output,
            &[
                (value as u8 & 0x7f) | 0x80,
                ((value >> 7) as u8 & 0x7f) | 0x80,
                (value >> 14) as u8,
            ],
            max_output,
        )
    }
}

#[cfg(feature = "solana-types")]
fn append_canonical(output: &mut Vec<u8>, bytes: &[u8], max_output: usize) -> Result<()> {
    let new_len = output
        .len()
        .checked_add(bytes.len())
        .ok_or(Error::DecodeLimitExceeded)?;
    if new_len > max_output {
        return Err(Error::DecodeLimitExceeded);
    }
    output
        .try_reserve(bytes.len())
        .map_err(|_| Error::DecodeLimitExceeded)?;
    output.extend_from_slice(bytes);
    Ok(())
}

/// Encodes reference Solana entries into the compact lencode shred format.
#[cfg(feature = "solana-types")]
pub struct SolanaEntryBatchEncoder {
    dictionary_id: [u8; SOLANA_DICTIONARY_ID_BYTES],
    context: EncoderContext,
    contextual: bool,
    leb128_address_ids: bool,
}

#[cfg(feature = "solana-types")]
impl SolanaEntryBatchEncoder {
    /// Creates an encoder backed by a frozen address dictionary.
    pub fn with_frozen(
        dictionary_id: [u8; SOLANA_DICTIONARY_ID_BYTES],
        frozen: Arc<FrozenEncoderState>,
    ) -> Self {
        Self {
            dictionary_id,
            context: EncoderContext {
                dedupe: Some(DedupeEncoder::with_frozen(frozen)),
                diff: None,
            },
            contextual: false,
            leb128_address_ids: false,
        }
    }

    /// Creates an encoder that shares novel address IDs across one entry batch.
    ///
    /// Context is reset at each batch boundary. A corrupted transaction makes
    /// the remainder of that batch undecodable, so consumers must reject the
    /// complete batch on any error.
    pub fn with_frozen_contextual(
        dictionary_id: [u8; SOLANA_DICTIONARY_ID_BYTES],
        frozen: Arc<FrozenEncoderState>,
    ) -> Self {
        Self {
            dictionary_id,
            context: EncoderContext {
                dedupe: Some(DedupeEncoder::with_frozen(frozen)),
                diff: None,
            },
            contextual: true,
            leb128_address_ids: false,
        }
    }

    /// Creates a contextual encoder with compact unsigned-LEB128 address IDs.
    ///
    /// This emits the explicit flag-8 frame variant. The transcoder continues
    /// to accept the older independent flag-2 and contextual flag-4 variants.
    pub fn with_frozen_contextual_leb128(
        dictionary_id: [u8; SOLANA_DICTIONARY_ID_BYTES],
        frozen: Arc<FrozenEncoderState>,
    ) -> Self {
        Self {
            dictionary_id,
            context: EncoderContext {
                dedupe: Some(DedupeEncoder::with_frozen_leb128(frozen)),
                diff: None,
            },
            contextual: true,
            leb128_address_ids: true,
        }
    }

    /// Encodes one exact entry batch into `output`, retaining its allocation.
    pub fn encode<'a>(
        &mut self,
        entries: impl ExactSizeIterator<Item = SolanaEntryRef<'a>>,
        output: &mut Vec<u8>,
    ) -> Result<usize> {
        output.clear();
        let mut writer = VecWriter(core::mem::take(output));
        let result = self.encode_inner(entries, &mut writer);
        *output = writer.into_inner();
        if result.is_err() {
            output.clear();
        }
        result
    }

    fn encode_inner<'a>(
        &mut self,
        entries: impl ExactSizeIterator<Item = SolanaEntryRef<'a>>,
        output: &mut VecWriter,
    ) -> Result<usize> {
        let entry_count = entries.len();
        if entry_count == 0 {
            return Err(Error::InvalidData);
        }
        output.0.extend_from_slice(&SOLANA_ENTRY_BATCH_MAGIC);
        output.0.push(SOLANA_ENTRY_BATCH_VERSION);
        output.0.push(if self.leb128_address_ids {
            SOLANA_ENTRY_BATCH_CONTEXTUAL_LEB128_DICTIONARY_FLAG
        } else if self.contextual {
            SOLANA_ENTRY_BATCH_CONTEXTUAL_DICTIONARY_FLAG
        } else {
            SOLANA_ENTRY_BATCH_DICTIONARY_FLAG
        });
        output.0.extend_from_slice(&self.dictionary_id);
        write_len(entry_count, output)?;

        if self.contextual {
            self.context
                .dedupe
                .as_mut()
                .expect("compact entry-batch encoder requires dedupe")
                .clear();
        }

        for entry in entries {
            entry.num_hashes.encode(output)?;
            output.0.extend_from_slice(entry.hash.as_bytes());
            write_len(entry.transactions.len(), output)?;
            for transaction in entry.transactions {
                if !self.contextual {
                    self.context
                        .dedupe
                        .as_mut()
                        .expect("compact entry-batch encoder requires dedupe")
                        .clear();
                }
                let frame_len_offset = output.0.len();
                output.0.extend_from_slice(&[0, 0]);
                let frame_start = output.0.len();
                transaction.encode_ext(output, Some(&mut self.context))?;
                let frame_len = output.0.len() - frame_start;
                if frame_len > MAX_LENCODE_TRANSACTION_BYTES {
                    return Err(Error::IncorrectLength);
                }
                let frame_len = u16::try_from(frame_len).map_err(|_| Error::IncorrectLength)?;
                output.0[frame_len_offset..frame_len_offset + 2]
                    .copy_from_slice(&frame_len.to_le_bytes());
            }
        }
        Ok(output.0.len())
    }
}

/// Reconstructs canonical Solana entry-batch bytes from compact lencode frames.
pub struct SolanaEntryBatchTranscoder {
    dictionary_id: [u8; SOLANA_DICTIONARY_ID_BYTES],
    transaction: SolanaTransactionTranscoder,
}

impl SolanaEntryBatchTranscoder {
    /// Creates a transcoder backed by the matching frozen address dictionary.
    pub fn with_frozen(
        dictionary_id: [u8; SOLANA_DICTIONARY_ID_BYTES],
        frozen: Arc<FrozenDecoderState>,
    ) -> Self {
        Self {
            dictionary_id,
            transaction: SolanaTransactionTranscoder::with_frozen(frozen),
        }
    }

    /// Reconstructs one exact compact batch and clears partial output on error.
    pub fn transcode_exact(
        &mut self,
        input: &[u8],
        output: &mut Vec<u8>,
        limits: EntryBatchWireLimits,
    ) -> Result<EntryBatchCounts> {
        output.clear();
        let result = self.transcode_inner(input, output, limits);
        if result.is_err() {
            output.clear();
        }
        result
    }

    fn transcode_inner(
        &mut self,
        input: &[u8],
        output: &mut Vec<u8>,
        limits: EntryBatchWireLimits,
    ) -> Result<EntryBatchCounts> {
        if input.len() > limits.max_input_bytes
            || input.len() < SOLANA_ENTRY_BATCH_HEADER_BYTES
            || input[..4] != SOLANA_ENTRY_BATCH_MAGIC
            || input[4] != SOLANA_ENTRY_BATCH_VERSION
            || input[6..SOLANA_ENTRY_BATCH_HEADER_BYTES] != self.dictionary_id
        {
            return Err(Error::InvalidData);
        }
        let (contextual, leb128_address_ids) = match input[5] {
            SOLANA_ENTRY_BATCH_DICTIONARY_FLAG => (false, false),
            SOLANA_ENTRY_BATCH_CONTEXTUAL_DICTIONARY_FLAG => (true, false),
            SOLANA_ENTRY_BATCH_CONTEXTUAL_LEB128_DICTIONARY_FLAG => (true, true),
            _ => return Err(Error::InvalidData),
        };
        self.transaction.leb128_address_ids = leb128_address_ids;
        if contextual {
            self.transaction.reset_context();
        }

        let body = &input[SOLANA_ENTRY_BATCH_HEADER_BYTES..];
        let decode_limits = DecodeLimits::new(
            body.len(),
            limits.max_sequence_len,
            limits.max_total_allocation,
        );
        let mut reader = LimitedReader::new(Cursor::new(body), decode_limits);
        let entry_count = read_len(&mut reader)?;
        if entry_count == 0 {
            return Err(Error::InvalidData);
        }
        reader.claim_sequence(entry_count, 1)?;
        append_u64(output, entry_count, limits.max_output_bytes)?;

        let mut transaction_total = 0usize;
        for _ in 0..entry_count {
            let num_hashes = u64::decode(&mut reader)?;
            append_growing_bytes(output, &num_hashes.to_le_bytes(), limits.max_output_bytes)?;
            append_growing_input(&mut reader, output, 32, limits.max_output_bytes)?;

            let transaction_count = read_len(&mut reader)?;
            reader.claim_sequence(transaction_count, 1)?;
            transaction_total = transaction_total
                .checked_add(transaction_count)
                .ok_or(Error::DecodeLimitExceeded)?;
            if transaction_total > limits.max_sequence_len {
                return Err(Error::DecodeLimitExceeded);
            }
            append_u64(output, transaction_count, limits.max_output_bytes)?;

            for _ in 0..transaction_count {
                let available = reader.buf().ok_or(Error::InvalidData)?;
                if available.len() < 2 {
                    return Err(Error::ReaderOutOfData);
                }
                let frame_len = usize::from(u16::from_le_bytes([available[0], available[1]]));
                reader.advance(2);
                if frame_len > MAX_LENCODE_TRANSACTION_BYTES {
                    return Err(Error::IncorrectLength);
                }
                let frame = reader.buf().ok_or(Error::InvalidData)?;
                if frame.len() < frame_len {
                    return Err(Error::ReaderOutOfData);
                }
                let remaining_output = limits
                    .max_output_bytes
                    .checked_sub(output.len())
                    .ok_or(Error::DecodeLimitExceeded)?;
                let mut transaction_limits = limits.transaction;
                transaction_limits.max_output_bytes =
                    transaction_limits.max_output_bytes.min(remaining_output);
                if contextual {
                    self.transaction.transcode_append_exact_continuing(
                        &frame[..frame_len],
                        output,
                        transaction_limits,
                    )?;
                } else {
                    self.transaction.transcode_append_exact(
                        &frame[..frame_len],
                        output,
                        transaction_limits,
                    )?;
                }
                reader.advance(frame_len);
            }
        }
        if reader.consumed() != body.len() {
            return Err(Error::TrailingData);
        }
        Ok(EntryBatchCounts {
            entries: entry_count,
            transactions: transaction_total,
        })
    }
}

/// Reconstructs canonical Solana transaction bytes into a reusable buffer.
///
/// Address dictionary entries must be primed as `[u8; 32]` in the same order
/// on the encoder and decoder. The ordinary entry points reset scratch dedupe
/// state for independently recoverable transactions; the contextual entry
/// point retains it until [`Self::reset_context`] is called.
pub struct SolanaTransactionTranscoder {
    decoder: DedupeDecoder,
    frozen_addresses: Option<Arc<Vec<[u8; 32]>>>,
    leb128_address_ids: bool,
    decompressed: Vec<u8>,
    address_offsets: Vec<usize>,
    direct_addresses: bool,
}

impl Default for SolanaTransactionTranscoder {
    fn default() -> Self {
        Self::new()
    }
}

impl SolanaTransactionTranscoder {
    /// Creates a transcoder without a frozen address dictionary.
    pub fn new() -> Self {
        Self {
            decoder: DedupeDecoder::new(),
            frozen_addresses: None,
            leb128_address_ids: false,
            decompressed: Vec::new(),
            address_offsets: Vec::new(),
            direct_addresses: false,
        }
    }

    /// Creates a transcoder backed by a frozen address dictionary.
    pub fn with_frozen(frozen: Arc<FrozenDecoderState>) -> Self {
        let decoder = DedupeDecoder::with_frozen(frozen);
        let frozen_addresses = decoder.shared_frozen_values::<[u8; 32]>();
        Self {
            decoder,
            frozen_addresses,
            leb128_address_ids: false,
            decompressed: Vec::new(),
            address_offsets: Vec::new(),
            direct_addresses: false,
        }
    }

    /// Returns the reusable dedupe decoder.
    pub const fn decoder(&self) -> &DedupeDecoder {
        &self.decoder
    }

    /// Transcodes exactly one lencode transaction frame.
    ///
    /// `output` is cleared but retains its allocation. On success it contains
    /// one canonical wincode transaction. On failure it is empty.
    pub fn transcode_exact(
        &mut self,
        input: &[u8],
        output: &mut Vec<u8>,
        limits: TransactionWireLimits,
    ) -> Result<usize> {
        output.clear();
        self.transcode_append_exact(input, output, limits)
    }

    /// Transcodes one exact frame and appends canonical bytes to `output`.
    ///
    /// Existing bytes and capacity are retained. If decoding fails, the
    /// output is rolled back to its original length.
    pub fn transcode_append_exact(
        &mut self,
        input: &[u8],
        output: &mut Vec<u8>,
        limits: TransactionWireLimits,
    ) -> Result<usize> {
        let output_start = output.len();
        self.decoder.clear();
        if input.len() > limits.max_input_bytes {
            return Err(Error::DecodeLimitExceeded);
        }
        let output_limit = output_start
            .checked_add(limits.max_output_bytes)
            .ok_or(Error::DecodeLimitExceeded)?;
        // One bounded reservation keeps every checked field append allocation-free.
        output
            .try_reserve(limits.max_output_bytes)
            .map_err(|_| Error::DecodeLimitExceeded)?;

        let decode_limits = DecodeLimits::new(
            limits.max_input_bytes,
            limits.max_sequence_len,
            limits.max_total_allocation,
        );
        let mut reader = LimitedReader::new(Cursor::new(input), decode_limits);
        self.address_offsets.clear();
        self.direct_addresses = true;
        let result = self.transcode_inner(&mut reader, output, output_limit, output_start);
        self.direct_addresses = false;
        if let Err(error) = result {
            output.truncate(output_start);
            return Err(error);
        }
        if reader.consumed() != input.len() {
            output.truncate(output_start);
            return Err(Error::TrailingData);
        }
        Ok(output.len() - output_start)
    }

    /// Clears transaction-local values while retaining the frozen address dictionary.
    ///
    /// Call this at the start of a contextual transaction stream before using
    /// [`Self::transcode_append_exact_continuing`].
    pub fn reset_context(&mut self) {
        self.decoder.clear();
        self.address_offsets.clear();
        self.direct_addresses = false;
    }

    /// Transcodes one exact frame without clearing values decoded from preceding frames.
    ///
    /// This is for an explicitly versioned outer frame whose encoder carries the same dedupe
    /// context across transactions. The caller must reset at the outer frame boundary and must
    /// discard the context after any error. Input and output limits still apply independently to
    /// every transaction.
    pub fn transcode_append_exact_continuing(
        &mut self,
        input: &[u8],
        output: &mut Vec<u8>,
        limits: TransactionWireLimits,
    ) -> Result<usize> {
        let output_start = output.len();
        if input.len() > limits.max_input_bytes {
            return Err(Error::DecodeLimitExceeded);
        }
        let output_limit = output_start
            .checked_add(limits.max_output_bytes)
            .ok_or(Error::DecodeLimitExceeded)?;
        output
            .try_reserve(limits.max_output_bytes)
            .map_err(|_| Error::DecodeLimitExceeded)?;

        let decode_limits = DecodeLimits::new(
            limits.max_input_bytes,
            limits.max_sequence_len,
            limits.max_total_allocation,
        );
        let mut reader = LimitedReader::new(Cursor::new(input), decode_limits);
        self.direct_addresses = true;
        let result = self.transcode_inner(&mut reader, output, output_limit, output_start);
        self.direct_addresses = false;
        if let Err(error) = result {
            output.truncate(output_start);
            return Err(error);
        }
        if reader.consumed() != input.len() {
            output.truncate(output_start);
            return Err(Error::TrailingData);
        }
        Ok(output.len() - output_start)
    }

    fn transcode_inner(
        &mut self,
        reader: &mut impl Read,
        output: &mut Vec<u8>,
        max_output: usize,
        output_start: usize,
    ) -> Result<()> {
        let signature_count = read_len(reader)?;
        reader.claim_sequence(signature_count, 64)?;
        let signature_bytes = signature_count
            .checked_mul(64)
            .ok_or(Error::DecodeLimitExceeded)?;

        let signature_prefix_len = append_short_u16(output, signature_count, max_output)?;
        append_input(reader, output, signature_bytes, max_output)?;

        let version = <usize as Decode>::decode_discriminant(reader)?;
        match version {
            0 => self.transcode_legacy_or_v0(reader, output, max_output, signature_count, false),
            1 => self.transcode_legacy_or_v0(reader, output, max_output, signature_count, true),
            2 => {
                // V1 canonical wire order is message followed by fixed-count
                // signatures. Remove the legacy/v0 signature length prefix,
                // append the message, then rotate in place.
                let address_offset_start = self.address_offsets.len();
                output.copy_within(output_start + signature_prefix_len.., output_start);
                output.truncate(output_start + signature_bytes);
                self.transcode_v1(reader, output, max_output, signature_count)?;
                output[output_start..].rotate_left(signature_bytes);
                for offset in &mut self.address_offsets[address_offset_start..] {
                    *offset = offset
                        .checked_sub(signature_bytes)
                        .ok_or(Error::InvalidData)?;
                }
                Ok(())
            }
            _ => Err(Error::InvalidData),
        }
    }

    fn transcode_legacy_or_v0(
        &mut self,
        reader: &mut impl Read,
        output: &mut Vec<u8>,
        max_output: usize,
        signature_count: usize,
        versioned: bool,
    ) -> Result<()> {
        if versioned {
            append_bytes(output, &[V0_PREFIX], max_output)?;
        }
        let header = read_header(reader)?;
        if usize::from(header[0]) != signature_count || (!versioned && header[0] & 0x80 != 0) {
            return Err(Error::InvalidData);
        }
        append_bytes(output, &header, max_output)?;
        self.transcode_addresses(reader, output, max_output)?;
        reader.claim_sequence(32, 1)?;
        append_input(reader, output, 32, max_output)?;
        self.transcode_instructions(reader, output, max_output)?;
        if versioned {
            self.transcode_address_table_lookups(reader, output, max_output)?;
        }
        Ok(())
    }

    fn transcode_v1(
        &mut self,
        reader: &mut impl Read,
        output: &mut Vec<u8>,
        max_output: usize,
        signature_count: usize,
    ) -> Result<()> {
        append_bytes(output, &[V1_PREFIX], max_output)?;
        let header = read_header(reader)?;
        if usize::from(header[0]) != signature_count {
            return Err(Error::InvalidData);
        }
        append_bytes(output, &header, max_output)?;

        let config_mask = u32::decode(reader)?;
        if config_mask & !0x1f != 0 || !matches!(config_mask & 0b11, 0 | 0b11) {
            return Err(Error::InvalidData);
        }
        append_bytes(output, &config_mask.to_le_bytes(), max_output)?;
        let config = read_v1_config(reader, config_mask)?;

        reader.claim_sequence(32, 1)?;
        append_input(reader, output, 32, max_output)?;

        let address_count = read_len(reader)?;
        let address_count_u8 = u8::try_from(address_count).map_err(|_| Error::IncorrectLength)?;
        reader.claim_sequence(address_count, 32)?;
        let counts_offset = output.len();
        append_bytes(output, &[0, address_count_u8], max_output)?;
        self.transcode_address_values(reader, output, max_output, address_count)?;
        append_v1_config(output, config, max_output)?;

        let instruction_count = read_len(reader)?;
        let instruction_count_u8 =
            u8::try_from(instruction_count).map_err(|_| Error::IncorrectLength)?;
        output[counts_offset] = instruction_count_u8;
        reader.claim_sequence(instruction_count, 1)?;

        let headers_len = instruction_count
            .checked_mul(4)
            .ok_or(Error::DecodeLimitExceeded)?;
        let headers_offset = output.len();
        append_zeroes(output, headers_len, max_output)?;

        for index in 0..instruction_count {
            let program_id_index = u8::decode(reader)?;
            let header_offset = headers_offset + index * 4;
            output[header_offset] = program_id_index;
            let accounts_len =
                append_byte_payload(reader, &mut self.decompressed, output, max_output, false)?;
            let accounts_len = u8::try_from(accounts_len).map_err(|_| Error::IncorrectLength)?;
            output[header_offset + 1] = accounts_len;
            let data_len =
                append_byte_payload(reader, &mut self.decompressed, output, max_output, false)?;
            let data_len = u16::try_from(data_len).map_err(|_| Error::IncorrectLength)?;
            output[header_offset + 2..header_offset + 4].copy_from_slice(&data_len.to_le_bytes());
        }
        Ok(())
    }

    fn transcode_addresses(
        &mut self,
        reader: &mut impl Read,
        output: &mut Vec<u8>,
        max_output: usize,
    ) -> Result<()> {
        let count = read_len(reader)?;
        reader.claim_sequence(count, 32)?;
        append_short_u16(output, count, max_output)?;
        self.transcode_address_values(reader, output, max_output, count)
    }

    fn transcode_address_values(
        &mut self,
        reader: &mut impl Read,
        output: &mut Vec<u8>,
        max_output: usize,
        count: usize,
    ) -> Result<()> {
        if self.direct_addresses {
            let frozen = self
                .frozen_addresses
                .as_deref()
                .map(Vec::as_slice)
                .or_else(|| (self.decoder.frozen_len() == 0).then_some(&[] as &[[u8; 32]]));
            if let Some(frozen) = frozen {
                return if self.leb128_address_ids {
                    Self::transcode_address_values_direct::<true>(
                        reader,
                        output,
                        max_output,
                        count,
                        frozen,
                        &mut self.address_offsets,
                    )
                } else {
                    Self::transcode_address_values_direct::<false>(
                        reader,
                        output,
                        max_output,
                        count,
                        frozen,
                        &mut self.address_offsets,
                    )
                };
            }
        }
        if self.leb128_address_ids {
            return Err(Error::InvalidData);
        }
        for _ in 0..count {
            let address = self.decoder.decode_ref::<[u8; 32]>(reader)?;
            append_bytes(output, address, max_output)?;
        }
        Ok(())
    }

    fn transcode_address_values_direct<const LEB128: bool>(
        reader: &mut impl Read,
        output: &mut Vec<u8>,
        max_output: usize,
        count: usize,
        frozen: &[[u8; 32]],
        offsets: &mut Vec<usize>,
    ) -> Result<()> {
        let output_bytes = count.checked_mul(32).ok_or(Error::DecodeLimitExceeded)?;
        ensure_output(output, output_bytes, max_output)?;
        if count == 0 {
            return Ok(());
        }

        let (consumed, novel_count) = {
            let available = reader.buf().ok_or(Error::InvalidData)?;
            let mut input = Cursor::new(available);
            let mut novel_count = 0usize;
            for _ in 0..count {
                let id = if LEB128 {
                    decode_leb128_usize(&mut input)?
                } else {
                    usize::try_from(Lencode::decode_varint_u64(&mut input)?)
                        .map_err(|_| Error::DecodeLimitExceeded)?
                };
                if id == 0 {
                    let available = input.buf().ok_or(Error::InvalidData)?;
                    if available.len() < 32 {
                        return Err(Error::ReaderOutOfData);
                    }
                    offsets.push(output.len());
                    output.extend_from_slice(&available[..32]);
                    input.advance(32);
                    novel_count += 1;
                } else if let Some(address) = frozen.get(id - 1) {
                    output.extend_from_slice(address);
                } else {
                    let index = id - frozen.len() - 1;
                    let offset = *offsets.get(index).ok_or(Error::InvalidData)?;
                    let end = offset.checked_add(32).ok_or(Error::DecodeLimitExceeded)?;
                    if end > output.len() {
                        return Err(Error::InvalidData);
                    }
                    output.extend_from_within(offset..end);
                }
            }
            (input.position(), novel_count)
        };
        if novel_count != 0 {
            reader.claim_allocation(novel_count * 32)?;
        }
        reader.advance(consumed);
        Ok(())
    }

    fn transcode_instructions(
        &mut self,
        reader: &mut impl Read,
        output: &mut Vec<u8>,
        max_output: usize,
    ) -> Result<()> {
        let count = read_len(reader)?;
        reader.claim_sequence(count, 1)?;
        append_short_u16(output, count, max_output)?;
        for _ in 0..count {
            if append_short_raw_instruction(reader, output, max_output)? {
                continue;
            }
            let program_id_index = u8::decode(reader)?;
            append_bytes(output, &[program_id_index], max_output)?;
            self.transcode_byte_vec(reader, output, max_output)?;
            self.transcode_byte_vec(reader, output, max_output)?;
        }
        Ok(())
    }

    fn transcode_address_table_lookups(
        &mut self,
        reader: &mut impl Read,
        output: &mut Vec<u8>,
        max_output: usize,
    ) -> Result<()> {
        let count = read_len(reader)?;
        reader.claim_sequence(count, 1)?;
        append_short_u16(output, count, max_output)?;
        for _ in 0..count {
            self.transcode_address_values(reader, output, max_output, 1)?;
            self.transcode_byte_vec(reader, output, max_output)?;
            self.transcode_byte_vec(reader, output, max_output)?;
        }
        Ok(())
    }

    fn transcode_byte_vec(
        &mut self,
        reader: &mut impl Read,
        output: &mut Vec<u8>,
        max_output: usize,
    ) -> Result<()> {
        append_byte_payload(reader, &mut self.decompressed, output, max_output, true)?;
        Ok(())
    }
}

#[inline(always)]
fn append_short_raw_instruction(
    reader: &mut impl Read,
    output: &mut Vec<u8>,
    max_output: usize,
) -> Result<bool> {
    let (consumed, accounts_len, data_len) = {
        let Some(input) = reader.buf() else {
            return Ok(false);
        };
        let Some(&accounts_flagged) = input.get(1) else {
            return Ok(false);
        };
        if accounts_flagged & 0x81 != 0 {
            return Ok(false);
        }
        let accounts_len = usize::from(accounts_flagged >> 1);
        let data_flag_offset = 2 + accounts_len;
        let Some(&data_flagged) = input.get(data_flag_offset) else {
            return Ok(false);
        };
        if data_flagged & 0x81 != 0 {
            return Ok(false);
        }
        let data_len = usize::from(data_flagged >> 1);
        let data_offset = data_flag_offset + 1;
        let consumed = data_offset + data_len;
        if consumed > input.len() {
            return Ok(false);
        }

        ensure_output(output, accounts_len + data_len + 3, max_output)?;
        let output_start = output.len();
        output.extend_from_slice(&input[..consumed]);
        output[output_start + 1] = accounts_len as u8;
        output[output_start + data_flag_offset] = data_len as u8;
        (consumed, accounts_len, data_len)
    };

    reader.claim_sequence(accounts_len, 1)?;
    reader.claim_sequence(data_len, 1)?;
    reader.advance(consumed);
    Ok(true)
}

fn append_byte_payload(
    reader: &mut impl Read,
    decompressed: &mut Vec<u8>,
    output: &mut Vec<u8>,
    max_output: usize,
    short_u16_prefix: bool,
) -> Result<usize> {
    let flagged = usize::try_from(Lencode::decode_varint_u64(reader)?)
        .map_err(|_| Error::DecodeLimitExceeded)?;
    let compressed = flagged & 1 == 1;
    let payload_len = flagged >> 1;
    if compressed {
        let original_len = {
            let available = reader.buf().ok_or(Error::InvalidData)?;
            if available.len() < payload_len {
                return Err(Error::ReaderOutOfData);
            }
            zstd_content_size(&available[..payload_len])?
        };
        reader.claim_sequence(original_len, 1)?;
        {
            let available = reader.buf().ok_or(Error::InvalidData)?;
            zstd_decompress_into(&available[..payload_len], original_len, decompressed)?;
        }
        reader.advance(payload_len);
        if short_u16_prefix {
            append_short_u16_payload(output, decompressed, max_output)?;
        } else {
            append_bytes(output, decompressed, max_output)?;
        }
        Ok(original_len)
    } else {
        reader.claim_sequence(payload_len, 1)?;
        {
            let available = reader.buf().ok_or(Error::InvalidData)?;
            if available.len() < payload_len {
                return Err(Error::ReaderOutOfData);
            }
            if short_u16_prefix {
                append_short_u16_payload(output, &available[..payload_len], max_output)?;
            } else {
                append_bytes(output, &available[..payload_len], max_output)?;
            }
        }
        reader.advance(payload_len);
        Ok(payload_len)
    }
}

#[derive(Clone, Copy)]
struct V1Config {
    priority_fee: Option<u64>,
    compute_unit_limit: Option<u32>,
    loaded_accounts_data_size_limit: Option<u32>,
    heap_size: Option<u32>,
}

fn read_v1_config(reader: &mut impl Read, mask: u32) -> Result<V1Config> {
    Ok(V1Config {
        priority_fee: (mask & 0b11 != 0)
            .then(|| u64::decode(reader))
            .transpose()?,
        compute_unit_limit: (mask & 0b100 != 0)
            .then(|| u32::decode(reader))
            .transpose()?,
        loaded_accounts_data_size_limit: (mask & 0b1000 != 0)
            .then(|| u32::decode(reader))
            .transpose()?,
        heap_size: (mask & 0b1_0000 != 0)
            .then(|| u32::decode(reader))
            .transpose()?,
    })
}

fn append_v1_config(output: &mut Vec<u8>, config: V1Config, max_output: usize) -> Result<()> {
    if let Some(value) = config.priority_fee {
        append_bytes(output, &value.to_le_bytes(), max_output)?;
    }
    if let Some(value) = config.compute_unit_limit {
        append_bytes(output, &value.to_le_bytes(), max_output)?;
    }
    if let Some(value) = config.loaded_accounts_data_size_limit {
        append_bytes(output, &value.to_le_bytes(), max_output)?;
    }
    if let Some(value) = config.heap_size {
        append_bytes(output, &value.to_le_bytes(), max_output)?;
    }
    Ok(())
}

fn read_header(reader: &mut impl Read) -> Result<[u8; 3]> {
    let bytes = u32::decode(reader)?.to_le_bytes();
    if bytes[3] != 0 {
        return Err(Error::InvalidData);
    }
    Ok([bytes[0], bytes[1], bytes[2]])
}

fn read_len(reader: &mut impl Read) -> Result<usize> {
    usize::try_from(Lencode::decode_varint_u64(reader)?).map_err(|_| Error::DecodeLimitExceeded)
}

#[inline(always)]
fn decode_leb128_usize(reader: &mut impl Read) -> Result<usize> {
    let input = reader.buf().ok_or(Error::InvalidData)?;
    let first = *input.first().ok_or(Error::ReaderOutOfData)?;
    if first < 0x80 {
        reader.advance(1);
        return Ok(usize::from(first));
    }

    let second = *input.get(1).ok_or(Error::ReaderOutOfData)?;
    let mut value = usize::from(first & 0x7f) | (usize::from(second & 0x7f) << 7);
    if second < 0x80 {
        reader.advance(2);
        return Ok(value);
    }

    let third = *input.get(2).ok_or(Error::ReaderOutOfData)?;
    value |= usize::from(third & 0x7f) << 14;
    if third < 0x80 {
        reader.advance(3);
        return Ok(value);
    }

    for (index, &byte) in input.iter().enumerate().skip(3) {
        let shift = index * 7;
        let payload = usize::from(byte & 0x7f);
        if shift >= usize::BITS as usize || payload > usize::MAX >> shift {
            return Err(Error::DecodeLimitExceeded);
        }
        value |= payload << shift;
        if byte < 0x80 {
            reader.advance(index + 1);
            return Ok(value);
        }
    }
    Err(Error::ReaderOutOfData)
}

#[cfg(feature = "solana-types")]
fn write_len(len: usize, output: &mut impl Write) -> Result<usize> {
    let len = u64::try_from(len).map_err(|_| Error::IncorrectLength)?;
    Lencode::encode_varint_u64(len, output)
}

fn append_u64(output: &mut Vec<u8>, value: usize, max_output: usize) -> Result<()> {
    let value = u64::try_from(value).map_err(|_| Error::IncorrectLength)?;
    append_growing_bytes(output, &value.to_le_bytes(), max_output)
}

fn append_growing_input(
    reader: &mut impl Read,
    output: &mut Vec<u8>,
    len: usize,
    max_output: usize,
) -> Result<()> {
    let available = reader.buf().ok_or(Error::InvalidData)?;
    if available.len() < len {
        return Err(Error::ReaderOutOfData);
    }
    append_growing_bytes(output, &available[..len], max_output)?;
    reader.advance(len);
    Ok(())
}

fn append_growing_bytes(output: &mut Vec<u8>, bytes: &[u8], max_output: usize) -> Result<()> {
    let new_len = output
        .len()
        .checked_add(bytes.len())
        .ok_or(Error::DecodeLimitExceeded)?;
    if new_len > max_output {
        return Err(Error::DecodeLimitExceeded);
    }
    output
        .try_reserve(bytes.len())
        .map_err(|_| Error::DecodeLimitExceeded)?;
    output.extend_from_slice(bytes);
    Ok(())
}

fn append_input(
    reader: &mut impl Read,
    output: &mut Vec<u8>,
    len: usize,
    max_output: usize,
) -> Result<()> {
    let available = reader.buf().ok_or(Error::InvalidData)?;
    if available.len() < len {
        return Err(Error::ReaderOutOfData);
    }
    append_bytes(output, &available[..len], max_output)?;
    reader.advance(len);
    Ok(())
}

fn append_short_u16(output: &mut Vec<u8>, len: usize, max_output: usize) -> Result<usize> {
    let value = u16::try_from(len).map_err(|_| Error::IncorrectLength)?;
    let bytes = if value < 0x80 {
        [value as u8, 0, 0]
    } else if value < 0x4000 {
        [(value as u8 & 0x7f) | 0x80, (value >> 7) as u8, 0]
    } else {
        [
            (value as u8 & 0x7f) | 0x80,
            ((value >> 7) as u8 & 0x7f) | 0x80,
            (value >> 14) as u8,
        ]
    };
    let encoded_len = if value < 0x80 {
        1
    } else if value < 0x4000 {
        2
    } else {
        3
    };
    append_bytes(output, &bytes[..encoded_len], max_output)?;
    Ok(encoded_len)
}

#[inline(always)]
fn append_short_u16_payload(output: &mut Vec<u8>, payload: &[u8], max_output: usize) -> Result<()> {
    if payload.len() < 0x80 {
        ensure_output(output, payload.len() + 1, max_output)?;
        output.push(payload.len() as u8);
        output.extend_from_slice(payload);
    } else {
        append_short_u16(output, payload.len(), max_output)?;
        append_bytes(output, payload, max_output)?;
    }
    Ok(())
}

fn append_zeroes(output: &mut Vec<u8>, len: usize, max_output: usize) -> Result<()> {
    ensure_output(output, len, max_output)?;
    output.resize(output.len() + len, 0);
    Ok(())
}

fn append_bytes(output: &mut Vec<u8>, bytes: &[u8], max_output: usize) -> Result<()> {
    ensure_output(output, bytes.len(), max_output)?;
    output.extend_from_slice(bytes);
    Ok(())
}

fn ensure_output(output: &mut Vec<u8>, additional: usize, max_output: usize) -> Result<()> {
    let new_len = output
        .len()
        .checked_add(additional)
        .ok_or(Error::DecodeLimitExceeded)?;
    if new_len > max_output {
        return Err(Error::DecodeLimitExceeded);
    }
    debug_assert!(new_len <= output.capacity());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Encode, dedupe::DedupeEncoder, io::VecWriter};
    use rand::{RngExt, SeedableRng, rngs::StdRng};
    use solana_hash::Hash;
    use solana_message::{
        Message as LegacyMessage, MessageHeader, VersionedMessage,
        compiled_instruction::CompiledInstruction,
        v0::{Message as V0Message, MessageAddressTableLookup},
        v1::{Message as V1Message, TransactionConfig},
    };
    use solana_pubkey::{Pubkey, PubkeyHasherBuilder};
    use solana_signature::Signature;
    use solana_transaction::versioned::VersionedTransaction;

    struct TestInstruction {
        program_id_index: u8,
        accounts: Vec<u8>,
        data: Vec<u8>,
    }

    struct TestLookup {
        address: [u8; 32],
        writable: Vec<u8>,
        readonly: Vec<u8>,
    }

    fn write_len(len: usize, writer: &mut impl crate::io::Write) {
        Lencode::encode_varint_u64(len as u64, writer).unwrap();
    }

    fn write_header(header: [u8; 3], writer: &mut impl crate::io::Write) {
        u32::from_le_bytes([header[0], header[1], header[2], 0])
            .encode(writer)
            .unwrap();
    }

    fn write_addresses(
        addresses: &[[u8; 32]],
        encoder: &mut DedupeEncoder,
        writer: &mut impl crate::io::Write,
    ) {
        write_len(addresses.len(), writer);
        for address in addresses {
            encoder
                .encode::<[u8; 32], crate::dedupe::DefaultDedupeHasher>(address, writer)
                .unwrap();
        }
    }

    fn write_instructions(instructions: &[TestInstruction], writer: &mut impl crate::io::Write) {
        write_len(instructions.len(), writer);
        for instruction in instructions {
            instruction.program_id_index.encode(writer).unwrap();
            instruction.accounts.encode(writer).unwrap();
            instruction.data.encode(writer).unwrap();
        }
    }

    fn write_signatures(signatures: &[[u8; 64]], writer: &mut impl crate::io::Write) {
        write_len(signatures.len(), writer);
        for signature in signatures {
            signature.encode(writer).unwrap();
        }
    }

    fn frozen_dictionary(
        addresses: &[[u8; 32]],
    ) -> (
        Arc<crate::dedupe::FrozenEncoderState>,
        Arc<FrozenDecoderState>,
    ) {
        let mut encoder = DedupeEncoder::new();
        let mut decoder = DedupeDecoder::new();
        for address in addresses {
            encoder.prime::<[u8; 32], crate::dedupe::DefaultDedupeHasher>(address);
            decoder.prime(*address);
        }
        (Arc::new(encoder.freeze()), Arc::new(decoder.freeze()))
    }

    fn frozen_reference_dictionary(
        addresses: &[[u8; 32]],
    ) -> (
        Arc<crate::dedupe::FrozenEncoderState>,
        Arc<FrozenDecoderState>,
    ) {
        let mut encoder = DedupeEncoder::new();
        let mut decoder = DedupeDecoder::new();
        for &address in addresses {
            encoder.prime::<Pubkey, PubkeyHasherBuilder>(&canonical_address(address));
            decoder.prime(address);
        }
        (Arc::new(encoder.freeze()), Arc::new(decoder.freeze()))
    }

    #[cfg(feature = "solana-types")]
    #[test]
    fn entry_batch_roundtrips_reference_transactions() {
        let addresses: Vec<[u8; 32]> = (0..300u16)
            .map(|index| {
                let mut address = [0u8; 32];
                address[..2].copy_from_slice(&index.to_le_bytes());
                address
            })
            .collect();
        let address = addresses[299];
        let (frozen_encoder, frozen_decoder) = frozen_reference_dictionary(&addresses);
        let transaction = VersionedTransaction {
            signatures: vec![Signature::from([1u8; 64])],
            message: VersionedMessage::Legacy(LegacyMessage {
                header: MessageHeader {
                    num_required_signatures: 1,
                    num_readonly_signed_accounts: 0,
                    num_readonly_unsigned_accounts: 0,
                },
                account_keys: vec![canonical_address(address)],
                recent_blockhash: Hash::new_from_array([3u8; 32]),
                instructions: Vec::new(),
            }),
        };
        let entry_hash = Hash::new_from_array([4u8; 32]);
        let dictionary_id = [5u8; SOLANA_DICTIONARY_ID_BYTES];
        let entries = [SolanaEntryRef {
            num_hashes: 7,
            hash: &entry_hash,
            transactions: core::slice::from_ref(&transaction),
        }];

        let mut old_encoder = SolanaEntryBatchEncoder::with_frozen_contextual(
            dictionary_id,
            Arc::clone(&frozen_encoder),
        );
        let mut old = Vec::new();
        old_encoder
            .encode(entries.iter().copied(), &mut old)
            .unwrap();
        let mut encoder =
            SolanaEntryBatchEncoder::with_frozen_contextual_leb128(dictionary_id, frozen_encoder);
        let mut compact = Vec::new();
        let compact_len = encoder.encode(entries.into_iter(), &mut compact).unwrap();
        assert_eq!(compact_len, compact.len());
        assert_eq!(old.len(), compact.len() + 1);
        assert_eq!(compact[..4], SOLANA_ENTRY_BATCH_MAGIC);
        assert_eq!(compact[4], SOLANA_ENTRY_BATCH_VERSION);
        assert_eq!(
            compact[5],
            SOLANA_ENTRY_BATCH_CONTEXTUAL_LEB128_DICTIONARY_FLAG
        );
        assert_eq!(compact[6..SOLANA_ENTRY_BATCH_HEADER_BYTES], dictionary_id);

        let canonical_transaction = wincode::serialize(&transaction).unwrap();
        let mut expected = Vec::new();
        expected.extend_from_slice(&1u64.to_le_bytes());
        expected.extend_from_slice(&7u64.to_le_bytes());
        expected.extend_from_slice(entry_hash.as_bytes());
        expected.extend_from_slice(&1u64.to_le_bytes());
        expected.extend_from_slice(&canonical_transaction);

        let limits = EntryBatchWireLimits::new(
            compact.len(),
            expected.len(),
            u16::MAX as usize,
            expected.len(),
            TransactionWireLimits::CURRENT,
        );
        let mut transcoder =
            SolanaEntryBatchTranscoder::with_frozen(dictionary_id, frozen_decoder.clone());
        let mut canonical = Vec::new();
        let counts = transcoder
            .transcode_exact(&compact, &mut canonical, limits)
            .unwrap();
        assert_eq!(canonical, expected);
        assert_eq!(
            counts,
            EntryBatchCounts {
                entries: 1,
                transactions: 1,
            }
        );

        canonical.extend_from_slice(&[9, 9, 9]);
        let mut wrong_dictionary = dictionary_id;
        wrong_dictionary[0] ^= 1;
        let error = SolanaEntryBatchTranscoder::with_frozen(wrong_dictionary, frozen_decoder)
            .transcode_exact(&compact, &mut canonical, limits)
            .unwrap_err();
        assert!(matches!(error, Error::InvalidData));
        assert!(canonical.is_empty());

        let mut trailing = compact.clone();
        trailing.push(0);
        let trailing_limits = EntryBatchWireLimits {
            max_input_bytes: trailing.len(),
            ..limits
        };
        let error = transcoder
            .transcode_exact(&trailing, &mut canonical, trailing_limits)
            .unwrap_err();
        assert!(matches!(error, Error::TrailingData));
        assert!(canonical.is_empty());

        let output_limited = EntryBatchWireLimits {
            max_output_bytes: expected.len() - 1,
            ..limits
        };
        let error = transcoder
            .transcode_exact(
                &trailing[..trailing.len() - 1],
                &mut canonical,
                output_limited,
            )
            .unwrap_err();
        assert!(matches!(error, Error::DecodeLimitExceeded));
        assert!(canonical.is_empty());

        let error = encoder
            .encode(core::iter::empty(), &mut compact)
            .unwrap_err();
        assert!(matches!(error, Error::InvalidData));
        assert!(compact.is_empty());
    }

    #[cfg(feature = "solana-types")]
    #[test]
    fn canonical_short_u16_sizes_cover_wire_boundaries() {
        for (len, expected) in [
            (0, 1),
            (0x7f, 1),
            (0x80, 2),
            (0x3fff, 2),
            (0x4000, 3),
            (usize::from(u16::MAX), 3),
        ] {
            assert_eq!(canonical_short_u16_serialized_size(len).unwrap(), expected);
        }
        assert!(matches!(
            canonical_short_u16_serialized_size(usize::from(u16::MAX) + 1),
            Err(Error::IncorrectLength)
        ));
    }

    #[cfg(feature = "solana-types")]
    #[test]
    fn canonical_prefixed_signature_sizes_cover_wire_boundaries() {
        for (len, prefix_size) in [
            (0, 1),
            (1, 1),
            (2, 1),
            (0x7f, 1),
            (0x80, 2),
            (0x3fff, 2),
            (0x4000, 3),
            (usize::from(u16::MAX), 3),
        ] {
            assert_eq!(
                canonical_prefixed_signatures_serialized_size(len).unwrap(),
                prefix_size + len * solana_signature::SIGNATURE_BYTES
            );
        }
        assert!(matches!(
            canonical_prefixed_signatures_serialized_size(usize::from(u16::MAX) + 1),
            Err(Error::IncorrectLength)
        ));
    }

    #[cfg(feature = "solana-types")]
    #[test]
    fn canonical_legacy_message_sizes_validate_account_key_width() {
        let account_key = canonical_address([2u8; 32]);
        let mut transaction = VersionedTransaction {
            signatures: Vec::new(),
            message: VersionedMessage::Legacy(LegacyMessage {
                header: MessageHeader {
                    num_required_signatures: 0,
                    num_readonly_signed_accounts: 0,
                    num_readonly_unsigned_accounts: 0,
                },
                account_keys: vec![account_key; usize::from(u16::MAX)],
                recent_blockhash: Hash::new_from_array([3u8; 32]),
                instructions: Vec::new(),
            }),
        };
        assert_eq!(
            canonical_solana_transaction_serialized_size(&transaction).unwrap(),
            usize::try_from(wincode::serialized_size(&transaction).unwrap()).unwrap()
        );

        let VersionedMessage::Legacy(message) = &mut transaction.message else {
            unreachable!();
        };
        message.account_keys.push(account_key);
        assert!(matches!(
            canonical_solana_transaction_serialized_size(&transaction),
            Err(Error::IncorrectLength)
        ));
    }

    #[cfg(feature = "solana-types")]
    #[test]
    fn canonical_instruction_sizes_cover_short_payload_boundaries() {
        for (accounts_len, data_len) in [
            (0, 0),
            (0x7f, 0x7f),
            (0x80, 0x7f),
            (0x7f, 0x80),
            (0x80, 0x80),
            (usize::from(u16::MAX), usize::from(u16::MAX)),
        ] {
            let instruction = CompiledInstruction {
                program_id_index: 0,
                accounts: vec![0; accounts_len],
                data: vec![0; data_len],
            };
            let expected = canonical_short_u16_serialized_size(1)
                .unwrap()
                .checked_add(1)
                .and_then(|size| {
                    size.checked_add(
                        canonical_short_payload_serialized_size(&instruction.accounts).unwrap(),
                    )
                })
                .and_then(|size| {
                    size.checked_add(
                        canonical_short_payload_serialized_size(&instruction.data).unwrap(),
                    )
                })
                .unwrap();
            assert_eq!(
                canonical_instructions_serialized_size(core::slice::from_ref(&instruction))
                    .unwrap(),
                u64::try_from(expected).unwrap()
            );
        }

        let mut maximal_count = vec![
            CompiledInstruction {
                program_id_index: 0,
                accounts: Vec::new(),
                data: Vec::new(),
            };
            usize::from(u16::MAX)
        ];
        let expected = canonical_short_u16_serialized_size(maximal_count.len())
            .unwrap()
            .checked_add(maximal_count.len().checked_mul(3).unwrap())
            .unwrap();
        assert_eq!(
            canonical_instructions_serialized_size(&maximal_count).unwrap(),
            u64::try_from(expected).unwrap()
        );
        maximal_count.push(CompiledInstruction {
            program_id_index: 0,
            accounts: Vec::new(),
            data: Vec::new(),
        });
        assert!(matches!(
            canonical_instructions_serialized_size(&maximal_count),
            Err(Error::IncorrectLength)
        ));

        let oversized = CompiledInstruction {
            program_id_index: 0,
            accounts: vec![0; usize::from(u16::MAX) + 1],
            data: Vec::new(),
        };
        assert!(matches!(
            canonical_instructions_serialized_size(core::slice::from_ref(&oversized)),
            Err(Error::IncorrectLength)
        ));
    }

    #[cfg(feature = "solana-types")]
    #[test]
    fn canonical_lookup_sizes_cover_short_payload_boundaries() {
        let header = MessageHeader {
            num_required_signatures: 0,
            num_readonly_signed_accounts: 0,
            num_readonly_unsigned_accounts: 0,
        };
        let account_key = canonical_address([2u8; 32]);
        for (writable_len, readonly_len) in [
            (0, 0),
            (0x7f, 0x7f),
            (0x80, 0x7f),
            (0x7f, 0x80),
            (0x80, 0x80),
            (usize::from(u16::MAX), 0),
            (0, usize::from(u16::MAX)),
        ] {
            let transaction = VersionedTransaction {
                signatures: Vec::new(),
                message: VersionedMessage::V0(V0Message {
                    header,
                    account_keys: Vec::new(),
                    recent_blockhash: Hash::new_from_array([3u8; 32]),
                    instructions: Vec::new(),
                    address_table_lookups: vec![MessageAddressTableLookup {
                        account_key,
                        writable_indexes: vec![0; writable_len],
                        readonly_indexes: vec![0; readonly_len],
                    }],
                }),
            };
            assert_eq!(
                canonical_solana_transaction_serialized_size(&transaction).unwrap(),
                usize::try_from(wincode::serialized_size(&transaction).unwrap()).unwrap()
            );
        }

        let mut maximal_count = VersionedTransaction {
            signatures: Vec::new(),
            message: VersionedMessage::V0(V0Message {
                header,
                account_keys: Vec::new(),
                recent_blockhash: Hash::new_from_array([4u8; 32]),
                instructions: Vec::new(),
                address_table_lookups: vec![
                    MessageAddressTableLookup {
                        account_key,
                        writable_indexes: Vec::new(),
                        readonly_indexes: Vec::new(),
                    };
                    usize::from(u16::MAX)
                ],
            }),
        };
        assert_eq!(
            canonical_solana_transaction_serialized_size(&maximal_count).unwrap(),
            usize::try_from(wincode::serialized_size(&maximal_count).unwrap()).unwrap()
        );
        let VersionedMessage::V0(message) = &mut maximal_count.message else {
            unreachable!();
        };
        message
            .address_table_lookups
            .push(MessageAddressTableLookup {
                account_key,
                writable_indexes: Vec::new(),
                readonly_indexes: Vec::new(),
            });
        assert!(matches!(
            canonical_solana_transaction_serialized_size(&maximal_count),
            Err(Error::IncorrectLength)
        ));

        let oversized = VersionedTransaction {
            signatures: Vec::new(),
            message: VersionedMessage::V0(V0Message {
                header,
                account_keys: Vec::new(),
                recent_blockhash: Hash::new_from_array([4u8; 32]),
                instructions: Vec::new(),
                address_table_lookups: vec![MessageAddressTableLookup {
                    account_key,
                    writable_indexes: vec![0; usize::from(u16::MAX) + 1],
                    readonly_indexes: Vec::new(),
                }],
            }),
        };
        assert!(matches!(
            canonical_solana_transaction_serialized_size(&oversized),
            Err(Error::IncorrectLength)
        ));
    }

    #[cfg(feature = "solana-types")]
    #[test]
    fn canonical_lz4_matches_current_wincode_and_enforces_limits() {
        let header = MessageHeader {
            num_required_signatures: 1,
            num_readonly_signed_accounts: 0,
            num_readonly_unsigned_accounts: 0,
        };
        let account_key = canonical_address([2u8; 32]);
        let instruction = CompiledInstruction {
            program_id_index: 0,
            accounts: vec![0],
            data: vec![3, 4],
        };
        let transactions = [
            VersionedTransaction {
                signatures: vec![Signature::from([5u8; 64])],
                message: VersionedMessage::Legacy(LegacyMessage {
                    header,
                    account_keys: vec![account_key],
                    recent_blockhash: Hash::new_from_array([6u8; 32]),
                    instructions: vec![instruction.clone()],
                }),
            },
            VersionedTransaction {
                signatures: vec![Signature::from([7u8; 64])],
                message: VersionedMessage::V0(V0Message {
                    header,
                    account_keys: vec![account_key],
                    recent_blockhash: Hash::new_from_array([8u8; 32]),
                    instructions: vec![instruction.clone()],
                    address_table_lookups: vec![MessageAddressTableLookup {
                        account_key,
                        writable_indexes: vec![1, 2],
                        readonly_indexes: vec![3],
                    }],
                }),
            },
            VersionedTransaction {
                signatures: vec![Signature::from([9u8; 64])],
                message: VersionedMessage::V1(V1Message {
                    header,
                    config: TransactionConfig::empty()
                        .with_priority_fee(10)
                        .with_compute_unit_limit(11)
                        .with_loaded_accounts_data_size_limit(12)
                        .with_heap_size(13),
                    lifetime_specifier: Hash::new_from_array([14u8; 32]),
                    account_keys: vec![account_key],
                    instructions: vec![instruction],
                }),
            },
        ];
        let entry_hash = Hash::new_from_array([15u8; 32]);
        let entries = [SolanaEntryRef {
            num_hashes: 16,
            hash: &entry_hash,
            transactions: &transactions,
        }];

        let mut expected = Vec::new();
        expected.extend_from_slice(&1u64.to_le_bytes());
        expected.extend_from_slice(&16u64.to_le_bytes());
        expected.extend_from_slice(entry_hash.as_bytes());
        expected.extend_from_slice(&3u64.to_le_bytes());
        for transaction in &transactions {
            expected.extend_from_slice(&wincode::serialize(transaction).unwrap());
            assert_eq!(
                canonical_solana_transaction_serialized_size(transaction).unwrap(),
                usize::try_from(wincode::serialized_size(transaction).unwrap()).unwrap()
            );
        }
        assert_eq!(
            canonical_solana_entry_serialized_size(entries[0]).unwrap(),
            expected.len() - size_of::<u64>()
        );
        assert_eq!(
            canonical_solana_entry_batch_serialized_size(entries.iter().copied()).unwrap(),
            expected.len()
        );
        assert_eq!(
            canonical_solana_entry_batch_serialized_size(core::iter::empty()).unwrap(),
            size_of::<u64>()
        );

        let mut invalid_v1 = transactions[2].clone();
        invalid_v1.signatures.clear();
        assert!(matches!(
            canonical_solana_transaction_serialized_size(&invalid_v1),
            Err(Error::InvalidData)
        ));

        let config = SolanaCanonicalLz4Config::new(expected.len(), expected.len());
        let mut encoder = SolanaCanonicalLz4EntryBatchEncoder::new(config);
        let mut encoded = Vec::new();
        let counts = encoder
            .encode(entries.iter().copied(), &mut encoded)
            .unwrap();
        assert_eq!(
            counts,
            EntryBatchCounts {
                entries: 1,
                transactions: 3,
            }
        );

        let compressed =
            lz4_block::compress(&expected, Some(CompressionMode::FAST(4)), true).unwrap();
        let mut expected_frame = SOLANA_ENTRY_BATCH_MAGIC.to_vec();
        expected_frame.extend_from_slice(&[
            SOLANA_ENTRY_BATCH_VERSION,
            SOLANA_ENTRY_BATCH_CANONICAL_LZ4_FLAG,
        ]);
        expected_frame.extend_from_slice(&compressed);
        assert_eq!(encoded, expected_frame);

        let stronger =
            lz4_block::compress(&expected, Some(CompressionMode::FAST(1)), true).unwrap();
        assert!(stronger.len() < compressed.len());
        let strongest = lz4_block::compress(
            &expected,
            Some(CompressionMode::HIGHCOMPRESSION(MID_HC_RECOMPRESSION_LEVEL)),
            true,
        )
        .unwrap();
        assert!(strongest.len() < stronger.len());
        encoder
            .encode_for_wire_blocks(
                entries.iter().copied(),
                &mut encoded,
                SolanaCanonicalLz4WireBlocks::new(1, 1),
            )
            .unwrap();
        expected_frame.truncate(SOLANA_CANONICAL_LZ4_HEADER_BYTES);
        expected_frame.extend_from_slice(&stronger);
        assert_eq!(encoded, expected_frame);

        encoder
            .encode_for_wire_blocks(
                entries.iter().copied(),
                &mut encoded,
                SolanaCanonicalLz4WireBlocks::new(5, 1),
            )
            .unwrap();
        expected_frame.truncate(SOLANA_CANONICAL_LZ4_HEADER_BYTES);
        expected_frame.extend_from_slice(&strongest);
        assert_eq!(encoded, expected_frame);

        let mut arena = Vec::new();
        let (arena_counts, frame) = encoder
            .encode_for_wire_blocks_in_arena(
                entries.iter().copied(),
                &mut arena,
                SolanaCanonicalLz4WireBlocks::new(5, 1),
            )
            .unwrap();
        assert_eq!(arena_counts, counts);
        assert_eq!(frame, expected_frame);

        let one_block = SolanaCanonicalLz4WireBlocks::new(1, usize::MAX);
        let (raw_counts, raw) = encoder
            .encode_for_wire_blocks_or_raw_in_arena(
                entries.iter().copied(),
                &mut arena,
                one_block,
                expected.len()..expected.len() + 1,
            )
            .unwrap();
        assert_eq!(raw_counts, counts);
        assert_eq!(raw, expected);

        let (_, upper_excluded) = encoder
            .encode_for_wire_blocks_or_raw_in_arena(
                entries.iter().copied(),
                &mut arena,
                one_block,
                0..expected.len(),
            )
            .unwrap();
        let mut fast_frame = SOLANA_ENTRY_BATCH_MAGIC.to_vec();
        fast_frame.extend_from_slice(&[
            SOLANA_ENTRY_BATCH_VERSION,
            SOLANA_ENTRY_BATCH_CANONICAL_LZ4_FLAG,
        ]);
        fast_frame.extend_from_slice(&compressed);
        assert_eq!(upper_excluded, fast_frame);

        let (_, saved_block) = encoder
            .encode_for_wire_blocks_or_raw_in_arena(
                entries.iter().copied(),
                &mut arena,
                SolanaCanonicalLz4WireBlocks::new(1, 1),
                0..usize::MAX,
            )
            .unwrap();
        assert!(saved_block.starts_with(&SOLANA_ENTRY_BATCH_MAGIC));

        let (_, invalid_blocks) = encoder
            .encode_for_wire_blocks_or_raw_in_arena(
                entries.iter().copied(),
                &mut arena,
                SolanaCanonicalLz4WireBlocks::new(0, 0),
                0..usize::MAX,
            )
            .unwrap();
        assert!(invalid_blocks.starts_with(&SOLANA_ENTRY_BATCH_MAGIC));

        let mut canonical = Vec::new();
        let decoded_len =
            transcode_canonical_lz4_entry_batch(&encoded, &mut canonical, expected.len()).unwrap();
        assert_eq!(decoded_len, expected.len());
        assert_eq!(canonical, expected);

        let decoder = SolanaCanonicalLz4EntryBatchDecoder::new(&encoded, expected.len()).unwrap();
        assert_eq!(decoder.canonical_len(), expected.len());
        let mut direct = vec![0; decoder.canonical_len()];
        assert_eq!(
            decoder.decompress_into(&mut direct).unwrap(),
            expected.len()
        );
        assert_eq!(direct, expected);

        let mut appended = vec![17, 18];
        assert_eq!(
            decoder.decompress_append_to_vec(&mut appended).unwrap(),
            expected.len()
        );
        assert_eq!(&appended[..2], &[17, 18]);
        assert_eq!(&appended[2..], expected);

        let mut shared = bytes::BytesMut::with_capacity(expected.len() + 2);
        shared.extend_from_slice(&[17, 18]);
        assert_eq!(
            decoder.decompress_append_to_bytes_mut(&mut shared).unwrap(),
            expected.len()
        );
        assert_eq!(&shared[..2], &[17, 18]);
        assert_eq!(&shared[2..], expected);

        let mut no_spare_capacity = bytes::BytesMut::new();
        assert!(matches!(
            decoder.decompress_append_to_bytes_mut(&mut no_spare_capacity),
            Err(Error::WriterOutOfSpace)
        ));
        assert!(no_spare_capacity.is_empty());

        let truncated = &encoded[..encoded.len() - 1];
        let truncated_decoder =
            SolanaCanonicalLz4EntryBatchDecoder::new(truncated, expected.len()).unwrap();
        let mut unchanged = bytes::BytesMut::with_capacity(expected.len() + 2);
        unchanged.extend_from_slice(&[17, 18]);
        assert!(matches!(
            truncated_decoder.decompress_append_to_bytes_mut(&mut unchanged),
            Err(Error::InvalidData)
        ));
        assert_eq!(&unchanged[..], &[17, 18]);

        assert!(matches!(
            decoder.decompress_into(&mut direct[..expected.len() - 1]),
            Err(Error::IncorrectLength)
        ));
        let mut negative_size = encoded.clone();
        negative_size[SOLANA_CANONICAL_LZ4_HEADER_BYTES..SOLANA_CANONICAL_LZ4_HEADER_BYTES + 4]
            .copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            SolanaCanonicalLz4EntryBatchDecoder::new(&negative_size, usize::MAX),
            Err(Error::IncorrectLength)
        ));

        let oversized_config = SolanaCanonicalLz4Config::new(expected.len(), expected.len() - 1);
        let mut oversized_encoder = SolanaCanonicalLz4EntryBatchEncoder::new(oversized_config);
        oversized_encoder
            .encode(entries.iter().copied(), &mut encoded)
            .unwrap();
        let compressed =
            lz4_block::compress(&expected, Some(CompressionMode::FAST(4)), true).unwrap();
        expected_frame.truncate(SOLANA_CANONICAL_LZ4_HEADER_BYTES);
        expected_frame.extend_from_slice(&compressed);
        assert_eq!(encoded, expected_frame);
        oversized_encoder
            .encode_for_wire_blocks(
                entries.iter().copied(),
                &mut encoded,
                SolanaCanonicalLz4WireBlocks::new(1, 1),
            )
            .unwrap();
        assert_eq!(encoded, expected_frame);

        let extreme_config = SolanaCanonicalLz4Config::new(expected.len(), expected.len() / 12);
        let mut extreme_encoder = SolanaCanonicalLz4EntryBatchEncoder::new(extreme_config);
        extreme_encoder
            .encode_for_wire_blocks(
                entries.iter().copied(),
                &mut encoded,
                SolanaCanonicalLz4WireBlocks::new(1, 1),
            )
            .unwrap();
        let compressed =
            lz4_block::compress(&expected, Some(CompressionMode::FAST(2)), true).unwrap();
        expected_frame.truncate(SOLANA_CANONICAL_LZ4_HEADER_BYTES);
        expected_frame.extend_from_slice(&compressed);
        assert_eq!(encoded, expected_frame);

        canonical.extend_from_slice(&[1, 2, 3]);
        transcode_canonical_lz4_entry_batch(&encoded, &mut canonical, expected.len()).unwrap();
        assert_eq!(canonical, expected);
        canonical.truncate(expected.len() / 2);
        transcode_canonical_lz4_entry_batch(&encoded, &mut canonical, expected.len()).unwrap();
        assert_eq!(canonical, expected);

        canonical.extend_from_slice(&[1, 2, 3]);
        assert!(matches!(
            transcode_canonical_lz4_entry_batch(&encoded, &mut canonical, expected.len() - 1,),
            Err(Error::DecodeLimitExceeded)
        ));
        assert!(canonical.is_empty());

        encoded[5] ^= 0x80;
        canonical.push(1);
        assert!(matches!(
            transcode_canonical_lz4_entry_batch(&encoded, &mut canonical, expected.len()),
            Err(Error::InvalidData)
        ));
        assert!(canonical.is_empty());

        let mut limited_encoder = SolanaCanonicalLz4EntryBatchEncoder::new(
            SolanaCanonicalLz4Config::new(expected.len() - 1, expected.len()),
        );
        assert!(matches!(
            limited_encoder.encode(entries.iter().copied(), &mut encoded),
            Err(Error::DecodeLimitExceeded)
        ));
        assert!(encoded.is_empty());
    }

    #[cfg(feature = "solana-types")]
    #[test]
    fn canonical_lz4_recompression_window_tracks_wire_blocks() {
        let blocks = SolanaCanonicalLz4WireBlocks::new(100, 80);
        assert_eq!(wire_block_count(80, blocks), Some(1));
        assert_eq!(wire_block_count(81, blocks), Some(2));
        assert_eq!(wire_block_count(180, blocks), Some(2));
        assert_eq!(wire_block_count(181, blocks), Some(3));
        assert!(should_recompress(81, blocks));
        assert!(!should_recompress(90, blocks));
        assert!(should_recompress(181, blocks));
        assert!(should_recompress(190, blocks));
        assert!(should_recompress(200, blocks));
        assert!(!should_recompress(201, blocks));
        assert_eq!(
            wire_block_count(181, SolanaCanonicalLz4WireBlocks::new(0, 80)),
            None
        );
    }

    #[cfg(feature = "solana-types")]
    #[test]
    fn canonical_lz4_decoder_bounds_advertised_expansion() {
        let mut impossible = SOLANA_ENTRY_BATCH_MAGIC.to_vec();
        impossible.extend_from_slice(&[
            SOLANA_ENTRY_BATCH_VERSION,
            SOLANA_ENTRY_BATCH_CANONICAL_LZ4_FLAG,
        ]);
        impossible.extend_from_slice(
            &u32::try_from(MAX_LZ4_BLOCK_COMPRESSION_RATIO + 1)
                .unwrap()
                .to_le_bytes(),
        );
        impossible.push(0);
        assert!(matches!(
            SolanaCanonicalLz4EntryBatchDecoder::new(
                &impossible,
                MAX_LZ4_BLOCK_COMPRESSION_RATIO + 1,
            ),
            Err(Error::InvalidData)
        ));

        let mut output = Vec::with_capacity(8);
        output.push(1);
        let original_capacity = output.capacity();
        assert!(matches!(
            transcode_canonical_lz4_entry_batch(
                &impossible,
                &mut output,
                MAX_LZ4_BLOCK_COMPRESSION_RATIO + 1,
            ),
            Err(Error::InvalidData)
        ));
        assert!(output.is_empty());
        assert_eq!(output.capacity(), original_capacity);

        let canonical = vec![0; 1 << 20];
        let compressed =
            lz4_block::compress(&canonical, Some(CompressionMode::FAST(4)), true).unwrap();
        let raw_compressed_len = compressed.len() - size_of::<u32>();
        assert!(canonical.len() > raw_compressed_len * 250);
        assert!(canonical.len() <= raw_compressed_len * MAX_LZ4_BLOCK_COMPRESSION_RATIO);
        let mut frame = SOLANA_ENTRY_BATCH_MAGIC.to_vec();
        frame.extend_from_slice(&[
            SOLANA_ENTRY_BATCH_VERSION,
            SOLANA_ENTRY_BATCH_CANONICAL_LZ4_FLAG,
        ]);
        frame.extend_from_slice(&compressed);
        transcode_canonical_lz4_entry_batch(&frame, &mut output, canonical.len()).unwrap();
        assert_eq!(output, canonical);
    }

    #[cfg(feature = "solana-types")]
    #[test]
    fn contextual_entry_batch_reuses_addresses_across_entries() {
        let address = canonical_address([6u8; 32]);
        let transaction = |signature: u8, blockhash: u8| VersionedTransaction {
            signatures: vec![Signature::from([signature; 64])],
            message: VersionedMessage::Legacy(LegacyMessage {
                header: MessageHeader {
                    num_required_signatures: 1,
                    num_readonly_signed_accounts: 0,
                    num_readonly_unsigned_accounts: 0,
                },
                account_keys: vec![address],
                recent_blockhash: Hash::new_from_array([blockhash; 32]),
                instructions: Vec::new(),
            }),
        };
        let first_transactions = [transaction(1, 2)];
        let second_transactions = [transaction(3, 4)];
        let first_hash = Hash::new_from_array([7u8; 32]);
        let second_hash = Hash::new_from_array([8u8; 32]);
        let entries = [
            SolanaEntryRef {
                num_hashes: 9,
                hash: &first_hash,
                transactions: &first_transactions,
            },
            SolanaEntryRef {
                num_hashes: 10,
                hash: &second_hash,
                transactions: &second_transactions,
            },
        ];
        let dictionary_id = [11u8; SOLANA_DICTIONARY_ID_BYTES];
        let (frozen_encoder, frozen_decoder) = frozen_dictionary(&[]);

        let mut independent_encoder =
            SolanaEntryBatchEncoder::with_frozen(dictionary_id, Arc::clone(&frozen_encoder));
        let mut independent = Vec::new();
        independent_encoder
            .encode(entries.iter().copied(), &mut independent)
            .unwrap();

        let mut contextual_encoder =
            SolanaEntryBatchEncoder::with_frozen_contextual(dictionary_id, frozen_encoder);
        let mut contextual = Vec::new();
        contextual_encoder
            .encode(entries.iter().copied(), &mut contextual)
            .unwrap();
        let mut contextual_again = Vec::new();
        contextual_encoder
            .encode(entries.iter().copied(), &mut contextual_again)
            .unwrap();

        assert_eq!(independent[5], SOLANA_ENTRY_BATCH_DICTIONARY_FLAG);
        assert_eq!(contextual[5], SOLANA_ENTRY_BATCH_CONTEXTUAL_DICTIONARY_FLAG);
        assert_eq!(independent.len() - contextual.len(), 32);
        assert_eq!(contextual_again, contextual);

        let mut expected = Vec::new();
        expected.extend_from_slice(&2u64.to_le_bytes());
        for (num_hashes, hash, transactions) in [
            (9u64, &first_hash, &first_transactions[..]),
            (10u64, &second_hash, &second_transactions[..]),
        ] {
            expected.extend_from_slice(&num_hashes.to_le_bytes());
            expected.extend_from_slice(hash.as_bytes());
            expected.extend_from_slice(&1u64.to_le_bytes());
            expected.extend_from_slice(&wincode::serialize(&transactions[0]).unwrap());
        }
        let limits = EntryBatchWireLimits::new(
            independent.len(),
            expected.len(),
            u16::MAX as usize,
            expected.len(),
            TransactionWireLimits::CURRENT,
        );
        let mut transcoder = SolanaEntryBatchTranscoder::with_frozen(dictionary_id, frozen_decoder);
        let mut canonical = Vec::new();
        transcoder
            .transcode_exact(&independent, &mut canonical, limits)
            .unwrap();
        assert_eq!(canonical, expected);
        transcoder
            .transcode_exact(&contextual, &mut canonical, limits)
            .unwrap();
        assert_eq!(canonical, expected);
        transcoder
            .transcode_exact(&contextual_again, &mut canonical, limits)
            .unwrap();
        assert_eq!(canonical, expected);

        let mut unknown = contextual;
        unknown[5] = 16;
        assert!(
            transcoder
                .transcode_exact(&unknown, &mut canonical, limits)
                .is_err()
        );
        assert!(canonical.is_empty());
    }

    #[test]
    fn leb128_address_ids_reject_truncation_and_overflow() {
        for input in [&[0x80][..], &[0xff; 10][..]] {
            let mut reader = Cursor::new(input);
            assert!(decode_leb128_usize(&mut reader).is_err());
        }
    }

    fn canonical_address(bytes: [u8; 32]) -> Pubkey {
        Pubkey::new_from_array(bytes)
    }

    fn canonical_instruction(instruction: &TestInstruction) -> CompiledInstruction {
        CompiledInstruction {
            program_id_index: instruction.program_id_index,
            accounts: instruction.accounts.clone(),
            data: instruction.data.clone(),
        }
    }

    fn assert_matches_current_wincode(expected: VersionedTransaction, actual: &[u8]) {
        assert_eq!(wincode::serialize(&expected).unwrap(), actual);
        let decoded: VersionedTransaction = wincode::deserialize(actual).unwrap();
        assert_eq!(decoded, expected);
    }

    fn random_bytes(rng: &mut StdRng, max_len: usize) -> Vec<u8> {
        (0..rng.random_range(0..=max_len))
            .map(|_| rng.random())
            .collect()
    }

    fn random_instructions(rng: &mut StdRng, address_count: usize) -> Vec<TestInstruction> {
        (0..rng.random_range(0..=4))
            .map(|_| TestInstruction {
                program_id_index: rng.random_range(0..address_count) as u8,
                accounts: (0..rng.random_range(0..=16))
                    .map(|_| rng.random_range(0..address_count) as u8)
                    .collect(),
                data: random_bytes(rng, 128),
            })
            .collect()
    }

    #[test]
    fn legacy_reconstructs_canonical_bytes_and_resets_scratch() {
        let signatures = [[1u8; 64]];
        let addresses = [[2u8; 32], [3u8; 32], [2u8; 32]];
        let instructions = [TestInstruction {
            program_id_index: 2,
            accounts: vec![0, 1],
            data: vec![9, 8, 7],
        }];
        let hash = [4u8; 32];
        let (frozen_encoder, frozen_decoder) = frozen_dictionary(&[addresses[0]]);
        let mut encoder = DedupeEncoder::with_frozen(frozen_encoder);
        let mut input = VecWriter::new();
        write_signatures(&signatures, &mut input);
        write_len(0, &mut input);
        write_header([1, 0, 1], &mut input);
        write_addresses(&addresses, &mut encoder, &mut input);
        hash.encode(&mut input).unwrap();
        write_instructions(&instructions, &mut input);

        let mut expected = Vec::new();
        expected.push(1);
        expected.extend_from_slice(&signatures[0]);
        expected.extend_from_slice(&[1, 0, 1, 3]);
        for address in addresses {
            expected.extend_from_slice(&address);
        }
        expected.extend_from_slice(&hash);
        expected.extend_from_slice(&[1, 2, 2, 0, 1, 3, 9, 8, 7]);

        let input = input.into_inner();
        let mut transcoder = SolanaTransactionTranscoder::with_frozen(frozen_decoder);
        let mut output = Vec::new();
        transcoder
            .transcode_exact(&input, &mut output, TransactionWireLimits::CURRENT)
            .unwrap();
        assert_eq!(output, expected);
        assert_matches_current_wincode(
            VersionedTransaction {
                signatures: signatures.map(Signature::from).to_vec(),
                message: VersionedMessage::Legacy(LegacyMessage {
                    header: MessageHeader {
                        num_required_signatures: 1,
                        num_readonly_signed_accounts: 0,
                        num_readonly_unsigned_accounts: 1,
                    },
                    account_keys: addresses.map(canonical_address).to_vec(),
                    recent_blockhash: Hash::new_from_array(hash),
                    instructions: instructions.iter().map(canonical_instruction).collect(),
                }),
            },
            &output,
        );

        let first_capacity = output.capacity();
        transcoder
            .transcode_exact(&input, &mut output, TransactionWireLimits::CURRENT)
            .unwrap();
        assert_eq!(output, expected);
        assert_eq!(output.capacity(), first_capacity);

        let prefix = [21, 22, 23];
        output.clear();
        output.extend_from_slice(&prefix);
        let appended = transcoder
            .transcode_append_exact(&input, &mut output, TransactionWireLimits::CURRENT)
            .unwrap();
        assert_eq!(appended, expected.len());
        assert_eq!(&output[..prefix.len()], &prefix);
        assert_eq!(&output[prefix.len()..], expected);
    }

    #[test]
    fn contextual_frames_share_addresses_until_reset() {
        let address = [31u8; 32];
        let mut encoder = DedupeEncoder::new();
        let encode_frame = |encoder: &mut DedupeEncoder, hash: [u8; 32]| {
            let mut input = VecWriter::new();
            write_signatures(&[], &mut input);
            write_len(0, &mut input);
            write_header([0, 0, 1], &mut input);
            write_addresses(&[address], encoder, &mut input);
            hash.encode(&mut input).unwrap();
            write_instructions(&[], &mut input);
            input.into_inner()
        };
        let first_hash = [32u8; 32];
        let second_hash = [33u8; 32];
        let first = encode_frame(&mut encoder, first_hash);
        let second = encode_frame(&mut encoder, second_hash);
        assert!(second.len() < first.len());

        let expected = |hash| {
            wincode::serialize(&VersionedTransaction {
                signatures: Vec::new(),
                message: VersionedMessage::Legacy(LegacyMessage {
                    header: MessageHeader {
                        num_required_signatures: 0,
                        num_readonly_signed_accounts: 0,
                        num_readonly_unsigned_accounts: 1,
                    },
                    account_keys: vec![canonical_address(address)],
                    recent_blockhash: Hash::new_from_array(hash),
                    instructions: Vec::new(),
                }),
            })
            .unwrap()
        };
        let expected_first = expected(first_hash);
        let expected_second = expected(second_hash);

        let mut transcoder = SolanaTransactionTranscoder::new();
        transcoder.reset_context();
        let mut output = Vec::new();
        let first_len = transcoder
            .transcode_append_exact_continuing(&first, &mut output, TransactionWireLimits::CURRENT)
            .unwrap();
        let second_len = transcoder
            .transcode_append_exact_continuing(&second, &mut output, TransactionWireLimits::CURRENT)
            .unwrap();
        assert_eq!(first_len, expected_first.len());
        assert_eq!(second_len, expected_second.len());
        assert_eq!(&output[..first_len], expected_first);
        assert_eq!(&output[first_len..], expected_second);

        transcoder.reset_context();
        let prefix = [1, 2, 3];
        output.clear();
        output.extend_from_slice(&prefix);
        assert!(
            transcoder
                .transcode_append_exact_continuing(
                    &second,
                    &mut output,
                    TransactionWireLimits::CURRENT,
                )
                .is_err()
        );
        assert_eq!(output, prefix);
    }

    #[test]
    fn contextual_v1_addresses_track_signature_reordering() {
        let address = [34u8; 32];
        let mut encoder = DedupeEncoder::new();
        let encode_frame = |encoder: &mut DedupeEncoder, signature: u8, hash: [u8; 32]| {
            let mut input = VecWriter::new();
            write_signatures(&[[signature; 64]], &mut input);
            write_len(2, &mut input);
            write_header([1, 0, 0], &mut input);
            0u32.encode(&mut input).unwrap();
            hash.encode(&mut input).unwrap();
            write_addresses(&[address], encoder, &mut input);
            write_instructions(&[], &mut input);
            input.into_inner()
        };
        let first_hash = [35u8; 32];
        let second_hash = [36u8; 32];
        let first = encode_frame(&mut encoder, 37, first_hash);
        let second = encode_frame(&mut encoder, 38, second_hash);
        assert_eq!(first.len() - second.len(), 32);

        let expected = |signature, hash| {
            wincode::serialize(&VersionedTransaction {
                signatures: vec![Signature::from([signature; 64])],
                message: VersionedMessage::V1(V1Message {
                    header: MessageHeader {
                        num_required_signatures: 1,
                        num_readonly_signed_accounts: 0,
                        num_readonly_unsigned_accounts: 0,
                    },
                    config: TransactionConfig::empty(),
                    lifetime_specifier: Hash::new_from_array(hash),
                    account_keys: vec![canonical_address(address)],
                    instructions: Vec::new(),
                }),
            })
            .unwrap()
        };
        let expected_first = expected(37, first_hash);
        let expected_second = expected(38, second_hash);

        let mut transcoder = SolanaTransactionTranscoder::new();
        transcoder.reset_context();
        let mut output = Vec::new();
        let first_len = transcoder
            .transcode_append_exact_continuing(&first, &mut output, TransactionWireLimits::CURRENT)
            .unwrap();
        let second_len = transcoder
            .transcode_append_exact_continuing(&second, &mut output, TransactionWireLimits::CURRENT)
            .unwrap();
        assert_eq!(first_len, expected_first.len());
        assert_eq!(second_len, expected_second.len());
        assert_eq!(&output[..first_len], expected_first);
        assert_eq!(&output[first_len..], expected_second);
    }

    #[test]
    fn v0_reconstructs_lookups_and_compressed_data() {
        let signatures = [[5u8; 64]];
        let frozen_address = [6u8; 32];
        let novel_address = [16u8; 32];
        let addresses = [frozen_address, novel_address];
        let hash = [7u8; 32];
        let instructions = [TestInstruction {
            program_id_index: 0,
            accounts: vec![0],
            data: vec![0; 512],
        }];
        let lookups = [TestLookup {
            address: novel_address,
            writable: vec![1, 2],
            readonly: vec![3],
        }];
        let (frozen_encoder, frozen_decoder) = frozen_dictionary(&[frozen_address]);
        let mut encoder = DedupeEncoder::with_frozen(frozen_encoder);
        let mut input = VecWriter::new();
        write_signatures(&signatures, &mut input);
        write_len(1, &mut input);
        write_header([1, 0, 0], &mut input);
        write_addresses(&addresses, &mut encoder, &mut input);
        hash.encode(&mut input).unwrap();
        write_instructions(&instructions, &mut input);
        write_len(lookups.len(), &mut input);
        for lookup in &lookups {
            encoder
                .encode::<[u8; 32], crate::dedupe::DefaultDedupeHasher>(&lookup.address, &mut input)
                .unwrap();
            lookup.writable.encode(&mut input).unwrap();
            lookup.readonly.encode(&mut input).unwrap();
        }

        let mut expected = Vec::new();
        expected.push(1);
        expected.extend_from_slice(&signatures[0]);
        expected.extend_from_slice(&[V0_PREFIX, 1, 0, 0, 2]);
        for address in addresses {
            expected.extend_from_slice(&address);
        }
        expected.extend_from_slice(&hash);
        expected.extend_from_slice(&[1, 0, 1, 0, 0x80, 0x04]);
        expected.extend_from_slice(&[0; 512]);
        expected.push(1);
        expected.extend_from_slice(&novel_address);
        expected.extend_from_slice(&[2, 1, 2, 1, 3]);

        let mut transcoder = SolanaTransactionTranscoder::with_frozen(frozen_decoder);
        let mut output = Vec::new();
        transcoder
            .transcode_exact(
                input.as_slice(),
                &mut output,
                TransactionWireLimits::CURRENT,
            )
            .unwrap();
        assert_eq!(output, expected);
        assert_matches_current_wincode(
            VersionedTransaction {
                signatures: signatures.map(Signature::from).to_vec(),
                message: VersionedMessage::V0(V0Message {
                    header: MessageHeader {
                        num_required_signatures: 1,
                        num_readonly_signed_accounts: 0,
                        num_readonly_unsigned_accounts: 0,
                    },
                    account_keys: addresses.map(canonical_address).to_vec(),
                    recent_blockhash: Hash::new_from_array(hash),
                    instructions: instructions.iter().map(canonical_instruction).collect(),
                    address_table_lookups: lookups
                        .iter()
                        .map(|lookup| MessageAddressTableLookup {
                            account_key: canonical_address(lookup.address),
                            writable_indexes: lookup.writable.clone(),
                            readonly_indexes: lookup.readonly.clone(),
                        })
                        .collect(),
                }),
            },
            &output,
        );
    }

    #[test]
    fn v1_reorders_message_headers_payloads_and_signatures() {
        let signatures = [[8u8; 64], [9u8; 64]];
        let addresses = [[10u8; 32], [11u8; 32]];
        let hash = [12u8; 32];
        let instructions = [
            TestInstruction {
                program_id_index: 1,
                accounts: vec![0, 1],
                data: vec![3, 4, 5],
            },
            TestInstruction {
                program_id_index: 1,
                accounts: vec![1],
                data: vec![6],
            },
        ];
        let (_, frozen_decoder) = frozen_dictionary(&[]);
        let mut encoder = DedupeEncoder::new();
        let mut input = VecWriter::new();
        write_signatures(&signatures, &mut input);
        write_len(2, &mut input);
        write_header([2, 1, 0], &mut input);
        0b1111u32.encode(&mut input).unwrap();
        42u64.encode(&mut input).unwrap();
        1_000u32.encode(&mut input).unwrap();
        2_000u32.encode(&mut input).unwrap();
        hash.encode(&mut input).unwrap();
        write_addresses(&addresses, &mut encoder, &mut input);
        write_instructions(&instructions, &mut input);

        let mut expected = Vec::new();
        expected.extend_from_slice(&[V1_PREFIX, 2, 1, 0]);
        expected.extend_from_slice(&0b1111u32.to_le_bytes());
        expected.extend_from_slice(&hash);
        expected.extend_from_slice(&[2, 2]);
        for address in addresses {
            expected.extend_from_slice(&address);
        }
        expected.extend_from_slice(&42u64.to_le_bytes());
        expected.extend_from_slice(&1_000u32.to_le_bytes());
        expected.extend_from_slice(&2_000u32.to_le_bytes());
        expected.extend_from_slice(&[1, 2]);
        expected.extend_from_slice(&3u16.to_le_bytes());
        expected.extend_from_slice(&[1, 1]);
        expected.extend_from_slice(&1u16.to_le_bytes());
        expected.extend_from_slice(&[0, 1, 3, 4, 5, 1, 6]);
        for signature in signatures {
            expected.extend_from_slice(&signature);
        }

        let mut transcoder = SolanaTransactionTranscoder::with_frozen(frozen_decoder);
        let mut output = Vec::new();
        transcoder
            .transcode_exact(
                input.as_slice(),
                &mut output,
                TransactionWireLimits::CURRENT,
            )
            .unwrap();
        assert_eq!(output, expected);
        assert_matches_current_wincode(
            VersionedTransaction {
                signatures: signatures.map(Signature::from).to_vec(),
                message: VersionedMessage::V1(V1Message {
                    header: MessageHeader {
                        num_required_signatures: 2,
                        num_readonly_signed_accounts: 1,
                        num_readonly_unsigned_accounts: 0,
                    },
                    config: TransactionConfig::empty()
                        .with_priority_fee(42)
                        .with_compute_unit_limit(1_000)
                        .with_loaded_accounts_data_size_limit(2_000),
                    lifetime_specifier: Hash::new_from_array(hash),
                    account_keys: addresses.map(canonical_address).to_vec(),
                    instructions: instructions.iter().map(canonical_instruction).collect(),
                }),
            },
            &output,
        );
    }

    #[test]
    fn rejects_trailing_data_and_output_expansion() {
        let mut input = VecWriter::new();
        write_signatures(&[], &mut input);
        write_len(0, &mut input);
        write_header([0, 0, 0], &mut input);
        write_addresses(&[], &mut DedupeEncoder::new(), &mut input);
        [0u8; 32].encode(&mut input).unwrap();
        write_instructions(&[], &mut input);
        input.0.push(99);

        let mut transcoder = SolanaTransactionTranscoder::new();
        let mut output = vec![1, 2, 3];
        assert!(matches!(
            transcoder.transcode_exact(
                input.as_slice(),
                &mut output,
                TransactionWireLimits::CURRENT,
            ),
            Err(Error::TrailingData)
        ));
        assert!(output.is_empty());

        input.0.pop();
        let limits = TransactionWireLimits {
            max_output_bytes: 8,
            ..TransactionWireLimits::CURRENT
        };
        assert!(matches!(
            transcoder.transcode_exact(input.as_slice(), &mut output, limits),
            Err(Error::DecodeLimitExceeded)
        ));
        assert!(output.is_empty());
    }

    #[test]
    fn randomized_frames_match_current_wincode() {
        let mut rng = StdRng::seed_from_u64(0x51_4f_4c_41_4e_41);
        for version in 0..=2 {
            for _ in 0..128 {
                let dictionary: Vec<[u8; 32]> = (0..4).map(|_| rng.random()).collect();
                let (frozen_encoder, frozen_decoder) = frozen_dictionary(&dictionary);
                let mut encoder = DedupeEncoder::with_frozen(frozen_encoder);
                let signature_count = rng.random_range(0..=3usize);
                let signatures: Vec<[u8; 64]> =
                    (0..signature_count).map(|_| rng.random()).collect();
                let address_count = rng.random_range(signature_count.max(1)..=16);
                let addresses: Vec<[u8; 32]> = (0..address_count)
                    .map(|_| {
                        if rng.random::<bool>() {
                            dictionary[rng.random_range(0..dictionary.len())]
                        } else {
                            rng.random()
                        }
                    })
                    .collect();
                let hash: [u8; 32] = rng.random();
                let instructions = random_instructions(&mut rng, address_count);
                let readonly_signed = rng.random_range(0..=signature_count) as u8;
                let readonly_unsigned = rng.random_range(0..=address_count - signature_count) as u8;
                let header = [signature_count as u8, readonly_signed, readonly_unsigned];

                let mut input = VecWriter::new();
                write_signatures(&signatures, &mut input);
                write_len(version, &mut input);
                write_header(header, &mut input);

                let message = match version {
                    0 => {
                        write_addresses(&addresses, &mut encoder, &mut input);
                        hash.encode(&mut input).unwrap();
                        write_instructions(&instructions, &mut input);
                        VersionedMessage::Legacy(LegacyMessage {
                            header: MessageHeader {
                                num_required_signatures: header[0],
                                num_readonly_signed_accounts: header[1],
                                num_readonly_unsigned_accounts: header[2],
                            },
                            account_keys: addresses
                                .iter()
                                .copied()
                                .map(canonical_address)
                                .collect(),
                            recent_blockhash: Hash::new_from_array(hash),
                            instructions: instructions.iter().map(canonical_instruction).collect(),
                        })
                    }
                    1 => {
                        write_addresses(&addresses, &mut encoder, &mut input);
                        hash.encode(&mut input).unwrap();
                        write_instructions(&instructions, &mut input);
                        let lookups: Vec<TestLookup> = (0..rng.random_range(0..=3))
                            .map(|_| TestLookup {
                                address: dictionary[rng.random_range(0..dictionary.len())],
                                writable: random_bytes(&mut rng, 8),
                                readonly: random_bytes(&mut rng, 8),
                            })
                            .collect();
                        write_len(lookups.len(), &mut input);
                        for lookup in &lookups {
                            encoder
                                .encode::<[u8; 32], crate::dedupe::DefaultDedupeHasher>(
                                    &lookup.address,
                                    &mut input,
                                )
                                .unwrap();
                            lookup.writable.encode(&mut input).unwrap();
                            lookup.readonly.encode(&mut input).unwrap();
                        }
                        VersionedMessage::V0(V0Message {
                            header: MessageHeader {
                                num_required_signatures: header[0],
                                num_readonly_signed_accounts: header[1],
                                num_readonly_unsigned_accounts: header[2],
                            },
                            account_keys: addresses
                                .iter()
                                .copied()
                                .map(canonical_address)
                                .collect(),
                            recent_blockhash: Hash::new_from_array(hash),
                            instructions: instructions.iter().map(canonical_instruction).collect(),
                            address_table_lookups: lookups
                                .iter()
                                .map(|lookup| MessageAddressTableLookup {
                                    account_key: canonical_address(lookup.address),
                                    writable_indexes: lookup.writable.clone(),
                                    readonly_indexes: lookup.readonly.clone(),
                                })
                                .collect(),
                        })
                    }
                    2 => {
                        let config = TransactionConfig {
                            priority_fee: rng.random::<bool>().then(|| rng.random()),
                            compute_unit_limit: rng
                                .random::<bool>()
                                .then(|| rng.random_range(1..=1_400_000)),
                            loaded_accounts_data_size_limit: rng
                                .random::<bool>()
                                .then(|| rng.random_range(1..=64 * 1024 * 1024)),
                            heap_size: rng
                                .random::<bool>()
                                .then(|| rng.random_range(32..=256) * 1024),
                        };
                        let mut mask = 0u32;
                        if config.priority_fee.is_some() {
                            mask |= 0b11;
                        }
                        if config.compute_unit_limit.is_some() {
                            mask |= 0b100;
                        }
                        if config.loaded_accounts_data_size_limit.is_some() {
                            mask |= 0b1000;
                        }
                        if config.heap_size.is_some() {
                            mask |= 0b1_0000;
                        }
                        mask.encode(&mut input).unwrap();
                        if let Some(value) = config.priority_fee {
                            value.encode(&mut input).unwrap();
                        }
                        if let Some(value) = config.compute_unit_limit {
                            value.encode(&mut input).unwrap();
                        }
                        if let Some(value) = config.loaded_accounts_data_size_limit {
                            value.encode(&mut input).unwrap();
                        }
                        if let Some(value) = config.heap_size {
                            value.encode(&mut input).unwrap();
                        }
                        hash.encode(&mut input).unwrap();
                        write_addresses(&addresses, &mut encoder, &mut input);
                        write_instructions(&instructions, &mut input);
                        VersionedMessage::V1(V1Message {
                            header: MessageHeader {
                                num_required_signatures: header[0],
                                num_readonly_signed_accounts: header[1],
                                num_readonly_unsigned_accounts: header[2],
                            },
                            config,
                            lifetime_specifier: Hash::new_from_array(hash),
                            account_keys: addresses
                                .iter()
                                .copied()
                                .map(canonical_address)
                                .collect(),
                            instructions: instructions.iter().map(canonical_instruction).collect(),
                        })
                    }
                    _ => unreachable!(),
                };

                let expected = VersionedTransaction {
                    signatures: signatures.into_iter().map(Signature::from).collect(),
                    message,
                };
                #[cfg(feature = "solana-types")]
                assert_eq!(
                    canonical_solana_transaction_serialized_size(&expected).unwrap(),
                    usize::try_from(wincode::serialized_size(&expected).unwrap()).unwrap()
                );
                let mut output = Vec::new();
                SolanaTransactionTranscoder::with_frozen(frozen_decoder)
                    .transcode_exact(
                        input.as_slice(),
                        &mut output,
                        TransactionWireLimits::CURRENT,
                    )
                    .unwrap();
                assert_matches_current_wincode(expected, &output);
            }
        }
    }

    #[test]
    fn malformed_frames_never_panic_and_roll_back_appends() {
        let mut rng = StdRng::seed_from_u64(0x42_4f_55_4e_44_53);
        let mut transcoder = SolanaTransactionTranscoder::new();
        for _ in 0..10_000 {
            let input = random_bytes(&mut rng, 512);
            let prefix = [1, 2, 3, 4, 5];
            let mut output = prefix.to_vec();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                transcoder.transcode_append_exact(
                    &input,
                    &mut output,
                    TransactionWireLimits::new(512, 4096, 512, 8192),
                )
            }));
            let result = result.expect("malformed compact frame panicked");
            if result.is_err() {
                assert_eq!(output, prefix);
            }
        }

        let mut input = VecWriter::new();
        write_signatures(&[[7u8; 64]], &mut input);
        write_len(0, &mut input);
        write_header([1, 0, 0], &mut input);
        write_addresses(&[[8u8; 32]], &mut DedupeEncoder::new(), &mut input);
        [9u8; 32].encode(&mut input).unwrap();
        write_instructions(&[], &mut input);
        for truncated_len in 0..input.as_slice().len() {
            let prefix = [6, 7, 8];
            let mut output = prefix.to_vec();
            assert!(
                transcoder
                    .transcode_append_exact(
                        &input.as_slice()[..truncated_len],
                        &mut output,
                        TransactionWireLimits::CURRENT,
                    )
                    .is_err()
            );
            assert_eq!(output, prefix);
        }
    }
}
