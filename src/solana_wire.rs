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
//! can enforce exact consumption and per-transaction recovery.
//!
//! With `solana-types`, `SolanaEntryBatchEncoder` accepts current reference
//! `VersionedTransaction` values and owns the complete compact `LCSH` batch
//! envelope. `SolanaEntryBatchTranscoder` validates that envelope and
//! reconstructs the canonical entry-batch bytes expected by Agave.

use std::{sync::Arc, vec::Vec};

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
use solana_hash::Hash as SolanaHash;
#[cfg(feature = "solana-types")]
use solana_transaction::versioned::VersionedTransaction;

/// Magic prefix for a compact Solana entry batch.
pub const SOLANA_ENTRY_BATCH_MAGIC: [u8; 4] = *b"LCSH";
/// Compact Solana entry-batch format version.
pub const SOLANA_ENTRY_BATCH_VERSION: u8 = 1;
/// Frame flag for native lencode transactions using an address dictionary.
pub const SOLANA_ENTRY_BATCH_DICTIONARY_FLAG: u8 = 2;
/// Bytes used to identify the frozen address dictionary.
pub const SOLANA_DICTIONARY_ID_BYTES: usize = 16;
/// Fixed compact entry-batch header size.
pub const SOLANA_ENTRY_BATCH_HEADER_BYTES: usize =
    SOLANA_ENTRY_BATCH_MAGIC.len() + 1 + 1 + SOLANA_DICTIONARY_ID_BYTES;

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

/// Encodes reference Solana entries into the compact lencode shred format.
#[cfg(feature = "solana-types")]
pub struct SolanaEntryBatchEncoder {
    dictionary_id: [u8; SOLANA_DICTIONARY_ID_BYTES],
    context: EncoderContext,
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
        output.0.push(SOLANA_ENTRY_BATCH_DICTIONARY_FLAG);
        output.0.extend_from_slice(&self.dictionary_id);
        write_len(entry_count, output)?;

        for entry in entries {
            entry.num_hashes.encode(output)?;
            output.0.extend_from_slice(entry.hash.as_bytes());
            write_len(entry.transactions.len(), output)?;
            for transaction in entry.transactions {
                self.context
                    .dedupe
                    .as_mut()
                    .expect("compact entry-batch encoder requires dedupe")
                    .clear();
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
            || input[5] != SOLANA_ENTRY_BATCH_DICTIONARY_FLAG
            || input[6..SOLANA_ENTRY_BATCH_HEADER_BYTES] != self.dictionary_id
        {
            return Err(Error::InvalidData);
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
                self.transaction.transcode_append_exact(
                    &frame[..frame_len],
                    output,
                    transaction_limits,
                )?;
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
/// on the encoder and decoder. Scratch dedupe state is reset for every call,
/// which keeps each transaction independently recoverable.
pub struct SolanaTransactionTranscoder {
    decoder: DedupeDecoder,
    decompressed: Vec<u8>,
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
            decompressed: Vec::new(),
        }
    }

    /// Creates a transcoder backed by a frozen address dictionary.
    pub fn with_frozen(frozen: Arc<FrozenDecoderState>) -> Self {
        Self {
            decoder: DedupeDecoder::with_frozen(frozen),
            decompressed: Vec::new(),
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
        let result = self.transcode_inner(&mut reader, output, output_limit, output_start);
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
        let result = self.transcode_inner(&mut reader, output, output_limit, output_start);
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
                output.copy_within(output_start + signature_prefix_len.., output_start);
                output.truncate(output_start + signature_bytes);
                self.transcode_v1(reader, output, max_output, signature_count)?;
                output[output_start..].rotate_left(signature_bytes);
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
        for _ in 0..count {
            let address = self.decoder.decode_ref::<[u8; 32]>(reader)?;
            append_bytes(output, address, max_output)?;
        }
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
            let address = self.decoder.decode_ref::<[u8; 32]>(reader)?;
            append_bytes(output, address, max_output)?;
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
            append_short_u16(output, original_len, max_output)?;
        }
        append_bytes(output, decompressed, max_output)?;
        Ok(original_len)
    } else {
        reader.claim_sequence(payload_len, 1)?;
        if short_u16_prefix {
            append_short_u16(output, payload_len, max_output)?;
        }
        {
            let available = reader.buf().ok_or(Error::InvalidData)?;
            if available.len() < payload_len {
                return Err(Error::ReaderOutOfData);
            }
            append_bytes(output, &available[..payload_len], max_output)?;
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
    use solana_pubkey::Pubkey;
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

    #[cfg(feature = "solana-types")]
    #[test]
    fn entry_batch_roundtrips_reference_transactions() {
        let address = [2u8; 32];
        let (frozen_encoder, frozen_decoder) = frozen_dictionary(&[address]);
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

        let mut encoder = SolanaEntryBatchEncoder::with_frozen(dictionary_id, frozen_encoder);
        let mut compact = Vec::new();
        let compact_len = encoder.encode(entries.into_iter(), &mut compact).unwrap();
        assert_eq!(compact_len, compact.len());
        assert_eq!(compact[..4], SOLANA_ENTRY_BATCH_MAGIC);
        assert_eq!(compact[4], SOLANA_ENTRY_BATCH_VERSION);
        assert_eq!(compact[5], SOLANA_ENTRY_BATCH_DICTIONARY_FLAG);
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
    fn v0_reconstructs_lookups_and_compressed_data() {
        let signatures = [[5u8; 64]];
        let address = [6u8; 32];
        let hash = [7u8; 32];
        let instructions = [TestInstruction {
            program_id_index: 0,
            accounts: vec![0],
            data: vec![0; 512],
        }];
        let lookups = [TestLookup {
            address,
            writable: vec![1, 2],
            readonly: vec![3],
        }];
        let (frozen_encoder, frozen_decoder) = frozen_dictionary(&[address]);
        let mut encoder = DedupeEncoder::with_frozen(frozen_encoder);
        let mut input = VecWriter::new();
        write_signatures(&signatures, &mut input);
        write_len(1, &mut input);
        write_header([1, 0, 0], &mut input);
        write_addresses(&[address], &mut encoder, &mut input);
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
        expected.extend_from_slice(&[V0_PREFIX, 1, 0, 0, 1]);
        expected.extend_from_slice(&address);
        expected.extend_from_slice(&hash);
        expected.extend_from_slice(&[1, 0, 1, 0, 0x80, 0x04]);
        expected.extend_from_slice(&[0; 512]);
        expected.push(1);
        expected.extend_from_slice(&address);
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
                    account_keys: vec![canonical_address(address)],
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
