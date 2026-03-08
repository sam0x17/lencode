//! Incremental binary diff encoding/decoding for byte blobs.
//!
//! [`DiffEncoder`] computes compact diffs between successive versions of a keyed
//! byte blob using two strategies and picks whichever is smaller:
//!
//! 1. **RLE patches** — run-length-encoded list of changed regions
//! 2. **XOR + zstd** — XOR the old and new blobs, then zstd-compress the result
//!    (mostly zeros with sparse non-zero bytes compress extremely well)
//!
//! [`DiffDecoder`] reconstructs the full blob from either format.
//!
//! Diff encoding is automatically wired into `Encode`/`Decode` for `Vec<u8>`,
//! `&[u8]`, `[u8; N]`, and `VecDeque<u8>` when an [`EncoderContext`]/[`DecoderContext`]
//! with an active diff key is provided via `encode_ext`/`decode_ext`.
//!
//! ## Wire format
//!
//! Each encoded blob starts with a varint **mode flag**:
//!
//! - `0` → full blob follows (varint length + raw bytes)
//! - `1` → RLE patch diff follows
//! - `2` → XOR + zstd diff follows
//!
//! RLE patch format (mode `1`):
//!
//! ```text
//! [new_len: varint]
//! [num_patches: varint]
//! for each patch:
//!     [gap: varint]        // bytes since end of last patch (or start of blob)
//!     [patch_len: varint]  // number of changed bytes
//!     [patch_data: bytes]  // the replacement bytes
//! ```
//!
//! XOR + zstd format (mode `2`):
//!
//! ```text
//! [new_len: varint]
//! [compressed_len: varint]
//! [compressed_xor: bytes]  // zstd frame of XOR(old, new), zero-padded if lengths differ
//! ```

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use hashbrown::HashMap;

use crate::bytes;
use crate::prelude::*;

/// A single contiguous region of changed bytes.
#[derive(Debug)]
struct Patch {
    /// Byte offset in the new blob where this patch starts.
    offset: usize,
    /// The changed bytes.
    data: Vec<u8>,
}

/// Minimum gap between patches before they get coalesced into one.
/// Coalescing avoids 2 varint headers (gap + len) when the gap is tiny.
const COALESCE_GAP: usize = 8;

/// Computes patches between `old` and `new` byte slices.
///
/// Adjacent patches separated by fewer than [`COALESCE_GAP`] bytes are merged.
/// Returns `None` if the patch data would exceed half the new blob size (full
/// blob is more compact in that case).
fn compute_patches(old: &[u8], new: &[u8]) -> Option<Vec<Patch>> {
    let min_len = old.len().min(new.len());
    let mut patches: Vec<Patch> = Vec::new();
    let mut i = 0;

    // Find differing regions in the overlapping prefix
    while i < min_len {
        if old[i] != new[i] {
            let start = i;
            // Scan to end of differing region
            while i < min_len && old[i] != new[i] {
                i += 1;
            }
            patches.push(Patch {
                offset: start,
                data: new[start..i].to_vec(),
            });
        } else {
            i += 1;
        }
    }

    // If new is longer, the tail is a patch
    if new.len() > old.len() {
        patches.push(Patch {
            offset: old.len(),
            data: new[old.len()..].to_vec(),
        });
    }

    // Coalesce nearby patches
    if patches.len() > 1 {
        let mut coalesced: Vec<Patch> = Vec::with_capacity(patches.len());
        coalesced.push(patches.remove(0));
        for p in patches {
            let last = coalesced.last_mut().unwrap();
            let last_end = last.offset + last.data.len();
            let gap = p.offset - last_end;
            if gap < COALESCE_GAP {
                // Merge: extend last patch to cover the gap + new patch
                last.data.extend_from_slice(&new[last_end..p.offset]);
                last.data.extend_from_slice(&p.data);
            } else {
                coalesced.push(p);
            }
        }
        // Check if patch data exceeds half the blob size
        let patch_bytes: usize = coalesced.iter().map(|p| p.data.len()).sum();
        if patch_bytes > new.len() / 2 {
            return None;
        }
        Some(coalesced)
    } else {
        // 0 or 1 patches — check size
        let patch_bytes: usize = patches.iter().map(|p| p.data.len()).sum();
        if patch_bytes > new.len() / 2 {
            return None;
        }
        Some(patches)
    }
}

/// Compute XOR of old and new, zero-padding for length differences.
/// Returns the XOR buffer (length = max(old.len(), new.len())).
fn compute_xor(old: &[u8], new: &[u8]) -> Vec<u8> {
    let max_len = old.len().max(new.len());
    let min_len = old.len().min(new.len());
    let mut xor = Vec::with_capacity(max_len);

    // XOR the overlapping region
    for i in 0..min_len {
        xor.push(old[i] ^ new[i]);
    }

    // Tail: XOR with 0 = identity, so just copy the longer tail
    if new.len() > old.len() {
        xor.extend_from_slice(&new[min_len..]);
    } else if old.len() > new.len() {
        xor.extend_from_slice(&old[min_len..]);
    }

    xor
}

/// Try XOR + zstd compression. Returns `None` if the compressed result
/// is not smaller than the full blob.
fn try_xor_compress(old: &[u8], new: &[u8]) -> Option<Vec<u8>> {
    let xor = compute_xor(old, new);
    let compressed = bytes::zstd_compress(&xor).ok()?;
    // Only use if smaller than raw blob + a small header margin
    if compressed.len() < new.len() {
        Some(compressed)
    } else {
        None
    }
}

/// Stateful encoder that produces compact diffs for keyed byte blobs.
///
/// Call [`set_key`](DiffEncoder::set_key) before encoding a blob to enable
/// delta encoding against the previously seen value for that key.
#[derive(Clone)]
pub struct DiffEncoder {
    /// Last seen blob per key.
    store: HashMap<u64, Vec<u8>>,
    /// Currently active key, if any.
    pub(crate) current_key: Option<u64>,
}

impl Default for DiffEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl DiffEncoder {
    /// Creates a new empty `DiffEncoder`.
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
            current_key: None,
        }
    }

    /// Creates a new `DiffEncoder` with pre-allocated capacity for `num_keys` keys.
    #[inline(always)]
    pub fn with_capacity(num_keys: usize) -> Self {
        Self {
            store: HashMap::with_capacity(num_keys),
            current_key: None,
        }
    }

    /// Sets the key for the next encode call.
    ///
    /// The key identifies which blob is being updated. Call this before each
    /// `encode_blob` to enable delta encoding against the last value seen for
    /// that key.
    #[inline(always)]
    pub const fn set_key(&mut self, key: u64) {
        self.current_key = Some(key);
    }

    /// Clears the active key.
    #[inline(always)]
    pub const fn clear_key(&mut self) {
        self.current_key = None;
    }

    /// Removes all cached blobs and resets the encoder.
    #[inline(always)]
    pub fn clear(&mut self) {
        self.store.clear();
        self.current_key = None;
    }

    /// Returns the number of keys with cached blobs.
    #[inline(always)]
    pub fn num_keys(&self) -> usize {
        self.store.len()
    }

    /// Returns `true` if a cached blob exists for the given key.
    #[inline(always)]
    pub fn contains_key(&self, key: u64) -> bool {
        self.store.contains_key(&key)
    }

    /// Returns an iterator over all cached keys.
    #[inline(always)]
    pub fn keys(&self) -> impl Iterator<Item = u64> + '_ {
        self.store.keys().copied()
    }

    /// Removes the cached blob for a specific key.
    ///
    /// The next encode for this key will emit a full blob instead of a diff.
    #[inline(always)]
    pub fn remove_key(&mut self, key: u64) {
        self.store.remove(&key);
    }

    /// Returns the total number of cached bytes across all keys.
    #[inline]
    pub fn cached_bytes(&self) -> usize {
        self.store.values().map(|v| v.len()).sum()
    }

    /// Returns an estimate of the heap memory (in bytes) used by the encoder.
    #[inline]
    pub fn memory_usage(&self) -> usize {
        use core::mem::size_of;
        // HashMap bucket overhead
        let map_overhead = self.store.capacity() * (size_of::<u64>() + size_of::<Vec<u8>>());
        // Actual cached blob data
        let blob_bytes: usize = self.store.values().map(|v| v.capacity()).sum();
        map_overhead + blob_bytes
    }

    /// Encodes a byte blob, producing a diff against the previously seen value
    /// for the current key (if any).
    ///
    /// Tries both RLE patches and XOR+zstd, picking whichever is smaller.
    /// Falls back to a full blob when neither strategy wins.
    ///
    /// Returns the number of bytes written.
    pub fn encode_blob(&mut self, data: &[u8], writer: &mut impl Write) -> Result<usize> {
        if let Some(key) = self.current_key {
            if let Some(old) = self.store.get(&key) {
                // Try RLE first (cheap)
                let rle_candidate = self.encode_rle_to_buf(old, data);

                // Skip the expensive XOR+zstd when RLE is already compact (< 10% of blob)
                let rle_is_tiny = rle_candidate
                    .as_ref()
                    .is_some_and(|buf| buf.len() * 10 <= data.len());

                let xor_candidate = if rle_is_tiny {
                    None
                } else {
                    self.encode_xor_to_buf(old, data)
                };

                let winner = match (&rle_candidate, &xor_candidate) {
                    (Some(rle), Some(xor)) => {
                        if rle.len() <= xor.len() {
                            rle_candidate.as_ref()
                        } else {
                            xor_candidate.as_ref()
                        }
                    }
                    (Some(_), None) => rle_candidate.as_ref(),
                    (None, Some(_)) => xor_candidate.as_ref(),
                    (None, None) => None,
                };

                if let Some(buf) = winner {
                    let n = writer.write(buf)?;
                    self.store.insert(key, data.to_vec());
                    return Ok(n);
                }
            }

            // No previous value or neither strategy wins — write full blob
            self.store.insert(key, data.to_vec());
        }

        // Full blob: mode flag 0 + length + data
        let mut total = 0;
        total += Lencode::encode_varint_u64(0, writer)?;
        total += Lencode::encode_varint_u64(data.len() as u64, writer)?;
        total += writer.write(data)?;
        Ok(total)
    }

    /// Encode RLE patches into a temporary buffer. Returns `None` if patches
    /// are too large (would exceed half the blob size).
    pub fn encode_rle_to_buf(&self, old: &[u8], new: &[u8]) -> Option<Vec<u8>> {
        let patches = compute_patches(old, new)?;
        let mut buf = Vec::new();
        // Mode 1 = RLE patches
        Lencode::encode_varint_u64(1, &mut buf).ok()?;
        Lencode::encode_varint_u64(new.len() as u64, &mut buf).ok()?;
        Lencode::encode_varint_u64(patches.len() as u64, &mut buf).ok()?;

        let mut cursor = 0usize;
        for patch in &patches {
            let gap = patch.offset - cursor;
            Lencode::encode_varint_u64(gap as u64, &mut buf).ok()?;
            Lencode::encode_varint_u64(patch.data.len() as u64, &mut buf).ok()?;
            buf.extend_from_slice(&patch.data);
            cursor = patch.offset + patch.data.len();
        }
        Some(buf)
    }

    /// Encode XOR+zstd into a temporary buffer. Returns `None` if the
    /// compressed result isn't smaller than the raw blob.
    pub fn encode_xor_to_buf(&self, old: &[u8], new: &[u8]) -> Option<Vec<u8>> {
        let compressed = try_xor_compress(old, new)?;
        let mut buf = Vec::new();
        // Mode 2 = XOR + zstd
        Lencode::encode_varint_u64(2, &mut buf).ok()?;
        Lencode::encode_varint_u64(new.len() as u64, &mut buf).ok()?;
        Lencode::encode_varint_u64(compressed.len() as u64, &mut buf).ok()?;
        buf.extend_from_slice(&compressed);
        Some(buf)
    }
}

/// Companion to [`DiffEncoder`] that reconstructs byte blobs from diffs.
///
/// Call [`set_key`](DiffDecoder::set_key) before decoding to match the key
/// used during encoding.
#[derive(Clone)]
pub struct DiffDecoder {
    /// Cached blobs per key.
    store: HashMap<u64, Vec<u8>>,
    /// Currently active key, if any.
    pub(crate) current_key: Option<u64>,
}

impl Default for DiffDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl DiffDecoder {
    /// Creates a new empty `DiffDecoder`.
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
            current_key: None,
        }
    }

    /// Creates a new `DiffDecoder` with pre-allocated capacity.
    #[inline(always)]
    pub fn with_capacity(num_keys: usize) -> Self {
        Self {
            store: HashMap::with_capacity(num_keys),
            current_key: None,
        }
    }

    /// Sets the key for the next decode call.
    #[inline(always)]
    pub const fn set_key(&mut self, key: u64) {
        self.current_key = Some(key);
    }

    /// Clears the active key.
    #[inline(always)]
    pub const fn clear_key(&mut self) {
        self.current_key = None;
    }

    /// Removes all cached blobs and resets the decoder.
    #[inline(always)]
    pub fn clear(&mut self) {
        self.store.clear();
        self.current_key = None;
    }

    /// Returns the number of keys with cached blobs.
    #[inline(always)]
    pub fn num_keys(&self) -> usize {
        self.store.len()
    }

    /// Returns `true` if a cached blob exists for the given key.
    #[inline(always)]
    pub fn contains_key(&self, key: u64) -> bool {
        self.store.contains_key(&key)
    }

    /// Returns an iterator over all cached keys.
    #[inline(always)]
    pub fn keys(&self) -> impl Iterator<Item = u64> + '_ {
        self.store.keys().copied()
    }

    /// Removes the cached blob for a specific key.
    ///
    /// The next decode for this key will expect a full blob.
    #[inline(always)]
    pub fn remove_key(&mut self, key: u64) {
        self.store.remove(&key);
    }

    /// Returns the total number of cached bytes across all keys.
    #[inline]
    pub fn cached_bytes(&self) -> usize {
        self.store.values().map(|v| v.len()).sum()
    }

    /// Returns an estimate of the heap memory (in bytes) used by the decoder.
    #[inline]
    pub fn memory_usage(&self) -> usize {
        use core::mem::size_of;
        let map_overhead = self.store.capacity() * (size_of::<u64>() + size_of::<Vec<u8>>());
        let blob_bytes: usize = self.store.values().map(|v| v.capacity()).sum();
        map_overhead + blob_bytes
    }

    /// Decodes a byte blob, applying patches if the stream contains a diff.
    ///
    /// Returns the reconstructed blob.
    pub fn decode_blob(&mut self, reader: &mut impl Read) -> Result<Vec<u8>> {
        let mode = Lencode::decode_varint_u64(reader)?;

        match mode {
            0 => {
                // Full blob
                let len = Lencode::decode_varint_u64(reader)? as usize;
                let mut data = Vec::with_capacity(len);
                if len > 0 {
                    unsafe { data.set_len(len) };
                    let n = reader.read(&mut data)?;
                    if n != len {
                        return Err(Error::ReaderOutOfData);
                    }
                }
                if let Some(key) = self.current_key {
                    self.store.insert(key, data.clone());
                }
                Ok(data)
            }
            1 => {
                // Patch diff — need old blob
                let new_len = Lencode::decode_varint_u64(reader)? as usize;
                let num_patches = Lencode::decode_varint_u64(reader)? as usize;

                let key = self.current_key.ok_or(Error::InvalidData)?;
                let old = self.store.get(&key).ok_or(Error::InvalidData)?;

                let mut result = Vec::with_capacity(new_len);
                let mut old_cursor = 0usize;

                for _ in 0..num_patches {
                    let gap = Lencode::decode_varint_u64(reader)? as usize;
                    let patch_len = Lencode::decode_varint_u64(reader)? as usize;

                    // Copy unchanged bytes from old blob
                    let copy_end = old_cursor + gap;
                    if copy_end > old.len() {
                        return Err(Error::InvalidData);
                    }
                    result.extend_from_slice(&old[old_cursor..copy_end]);

                    // Read patch data
                    let start = result.len();
                    result.resize(start + patch_len, 0);
                    let n = reader.read(&mut result[start..start + patch_len])?;
                    if n != patch_len {
                        return Err(Error::ReaderOutOfData);
                    }

                    old_cursor = copy_end + patch_len;
                }

                // Copy any remaining unchanged tail from old blob
                // (only valid up to min(old.len(), new_len) since new might be shorter)
                let remaining = old.len().min(new_len) - old_cursor.min(old.len().min(new_len));
                if remaining > 0 && old_cursor < old.len() {
                    let tail_end = old_cursor + remaining;
                    result.extend_from_slice(&old[old_cursor..tail_end.min(old.len())]);
                }

                if result.len() != new_len {
                    return Err(Error::InvalidData);
                }

                self.store.insert(key, result.clone());
                Ok(result)
            }
            2 => {
                // XOR + zstd diff
                let new_len = Lencode::decode_varint_u64(reader)? as usize;
                let compressed_len = Lencode::decode_varint_u64(reader)? as usize;

                let key = self.current_key.ok_or(Error::InvalidData)?;
                let old = self.store.get(&key).ok_or(Error::InvalidData)?;

                // Read compressed XOR data
                let mut compressed = Vec::with_capacity(compressed_len);
                if compressed_len > 0 {
                    unsafe { compressed.set_len(compressed_len) };
                    let n = reader.read(&mut compressed)?;
                    if n != compressed_len {
                        return Err(Error::ReaderOutOfData);
                    }
                }

                // Decompress the XOR buffer
                let xor_len = old.len().max(new_len);
                let xor = bytes::zstd_decompress(&compressed, xor_len)?;

                // Reconstruct: new[i] = old[i] ^ xor[i] for overlapping region
                let mut result = Vec::with_capacity(new_len);
                let min_len = old.len().min(new_len);
                for i in 0..min_len {
                    result.push(old[i] ^ xor[i]);
                }
                // If new is longer, tail of XOR is the new bytes (XOR with 0 = identity)
                if new_len > old.len() {
                    result.extend_from_slice(&xor[min_len..new_len]);
                }

                if result.len() != new_len {
                    return Err(Error::InvalidData);
                }

                self.store.insert(key, result.clone());
                Ok(result)
            }
            _ => Err(Error::InvalidData),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::Cursor;

    #[test]
    fn test_diff_full_blob_roundtrip() {
        let mut encoder = DiffEncoder::new();
        let mut decoder = DiffDecoder::new();
        let mut buf = Vec::new();

        let data = b"hello world";
        encoder.encode_blob(data, &mut buf).unwrap();

        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_diff_keyed_patch_roundtrip() {
        let mut encoder = DiffEncoder::new();
        let mut decoder = DiffDecoder::new();
        let mut buf = Vec::new();

        let key = 42u64;

        // First write: full blob
        encoder.set_key(key);
        decoder.set_key(key);
        let data1 = b"hello world, this is a test of the diff encoder!";
        encoder.encode_blob(data1, &mut buf).unwrap();

        let mut cursor = Cursor::new(&buf[..]);
        let result1 = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result1, data1);

        // Second write: small diff
        buf.clear();
        encoder.set_key(key);
        decoder.set_key(key);
        let data2 = b"hello World, this is a test of the diff encoder!";
        //                  ^ capital W
        encoder.encode_blob(data2, &mut buf).unwrap();

        // The diff should be smaller than the full blob
        assert!(
            buf.len() < data2.len(),
            "diff should be smaller: {} vs {}",
            buf.len(),
            data2.len()
        );

        let mut cursor = Cursor::new(&buf[..]);
        let result2 = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result2, data2);
    }

    #[test]
    fn test_diff_multiple_small_changes() {
        let mut encoder = DiffEncoder::new();
        let mut decoder = DiffDecoder::new();
        let key = 1u64;

        // Start with a 1KB blob
        let mut data = vec![0u8; 1024];
        for (i, b) in data.iter_mut().enumerate() {
            *b = (i % 256) as u8;
        }

        let mut buf = Vec::new();
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&data, &mut buf).unwrap();
        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data);

        // Change a few bytes
        data[100] = 0xFF;
        data[500] = 0xAB;
        data[900] = 0xCD;

        buf.clear();
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&data, &mut buf).unwrap();

        // Should be much smaller than full blob
        assert!(
            buf.len() < 50,
            "patch should be very small: {} bytes",
            buf.len()
        );

        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_diff_size_change() {
        let mut encoder = DiffEncoder::new();
        let mut decoder = DiffDecoder::new();
        let key = 7u64;

        // Initial blob
        let data1 = vec![1u8; 100];
        let mut buf = Vec::new();
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&data1, &mut buf).unwrap();
        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data1);

        // Larger blob (appended data)
        let mut data2 = vec![1u8; 100];
        data2.extend_from_slice(&[2u8; 50]);
        buf.clear();
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&data2, &mut buf).unwrap();
        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data2);

        // Shorter blob (truncated)
        let data3 = vec![1u8; 80];
        buf.clear();
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&data3, &mut buf).unwrap();
        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data3);
    }

    #[test]
    fn test_diff_no_key_always_full() {
        let mut encoder = DiffEncoder::new();
        let mut decoder = DiffDecoder::new();
        let mut buf = Vec::new();

        let data = b"test data";
        encoder.encode_blob(data, &mut buf).unwrap();

        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data);

        // Without a key, second write is also full
        buf.clear();
        encoder.encode_blob(data, &mut buf).unwrap();
        // Mode byte should be 0 (full)
        assert_eq!(buf[0], 0);
    }

    #[test]
    fn test_diff_identical_blob() {
        let mut encoder = DiffEncoder::new();
        let mut decoder = DiffDecoder::new();
        let key = 99u64;

        let data = vec![42u8; 256];
        let mut buf = Vec::new();

        // First encode (full)
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&data, &mut buf).unwrap();
        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data);

        // Second encode (identical data → zero patches)
        buf.clear();
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&data, &mut buf).unwrap();

        // Should be very small: mode(1) + new_len(2) + num_patches(1) = ~4 bytes
        assert!(
            buf.len() < 10,
            "identical blob diff should be tiny: {} bytes",
            buf.len()
        );

        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_diff_via_vec_u8_encode_decode() {
        use crate::context::{DecoderContext, EncoderContext};
        use crate::{Decode, Encode};

        let key = 42u64;

        // Create contexts with diff enabled
        let mut enc_ctx = EncoderContext {
            dedupe: None,
            diff: Some(DiffEncoder::new()),
        };
        let mut dec_ctx = DecoderContext {
            dedupe: None,
            diff: Some(DiffDecoder::new()),
        };

        // First encode: full blob through Vec<u8> Encode trait
        let data1: Vec<u8> = (0..200).collect();
        let mut buf = Vec::new();
        enc_ctx.diff.as_mut().unwrap().set_key(key);
        dec_ctx.diff.as_mut().unwrap().set_key(key);
        data1.encode_ext(&mut buf, Some(&mut enc_ctx)).unwrap();

        let mut cursor = Cursor::new(&buf[..]);
        let result1: Vec<u8> = Vec::decode_ext(&mut cursor, Some(&mut dec_ctx)).unwrap();
        assert_eq!(result1, data1);

        // Second encode: small diff (change 2 bytes)
        let mut data2 = data1.clone();
        data2[50] = 0xFF;
        data2[150] = 0xFE;
        buf.clear();
        enc_ctx.diff.as_mut().unwrap().set_key(key);
        dec_ctx.diff.as_mut().unwrap().set_key(key);
        data2.encode_ext(&mut buf, Some(&mut enc_ctx)).unwrap();

        // Diff should be much smaller than full blob
        assert!(
            buf.len() < data2.len() / 2,
            "diff should be compact: {} vs {}",
            buf.len(),
            data2.len()
        );

        let mut cursor = Cursor::new(&buf[..]);
        let result2: Vec<u8> = Vec::decode_ext(&mut cursor, Some(&mut dec_ctx)).unwrap();
        assert_eq!(result2, data2);
    }

    #[test]
    fn test_diff_xor_roundtrip_scattered_changes() {
        // Scattered changes across a large blob should trigger XOR+zstd (mode 2)
        let mut encoder = DiffEncoder::new();
        let mut decoder = DiffDecoder::new();
        let key = 200u64;

        // 4KB blob
        let data1: Vec<u8> = (0..4096).map(|i| (i % 256) as u8).collect();
        let mut buf = Vec::new();
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&data1, &mut buf).unwrap();
        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data1);

        // Scatter ~40% of bytes (well above RLE half-blob cutoff, should use XOR)
        let mut data2 = data1.clone();
        for i in (0..data2.len()).step_by(3) {
            data2[i] = data2[i].wrapping_add(1);
        }
        buf.clear();
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&data2, &mut buf).unwrap();

        // Verify mode byte is 2 (XOR+zstd)
        assert_eq!(
            buf[0], 2,
            "expected mode 2 (XOR+zstd) for scattered changes"
        );

        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data2);
    }

    #[test]
    fn test_diff_rle_mode_for_small_changes() {
        // A single small change should use RLE (mode 1)
        let mut encoder = DiffEncoder::new();
        let key = 300u64;

        let data1 = vec![0xAAu8; 2048];
        let mut buf = Vec::new();
        encoder.set_key(key);
        encoder.encode_blob(&data1, &mut buf).unwrap();

        let mut data2 = data1.clone();
        data2[1000] = 0xBB;
        buf.clear();
        encoder.set_key(key);
        encoder.encode_blob(&data2, &mut buf).unwrap();

        assert_eq!(buf[0], 1, "expected mode 1 (RLE) for single byte change");
    }

    #[test]
    fn test_diff_xor_with_append() {
        // XOR path with new blob longer than old
        let mut encoder = DiffEncoder::new();
        let mut decoder = DiffDecoder::new();
        let key = 400u64;

        let data1: Vec<u8> = (0..2048).map(|i| (i % 256) as u8).collect();
        let mut buf = Vec::new();
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&data1, &mut buf).unwrap();
        let mut cursor = Cursor::new(&buf[..]);
        decoder.decode_blob(&mut cursor).unwrap();

        // Scatter changes AND append 512 bytes
        let mut data2 = data1.clone();
        for i in (0..data2.len()).step_by(3) {
            data2[i] = data2[i].wrapping_add(5);
        }
        data2.extend_from_slice(&[0xCC; 512]);

        buf.clear();
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&data2, &mut buf).unwrap();

        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data2);
    }

    #[test]
    fn test_diff_xor_with_truncate() {
        // XOR path with new blob shorter than old
        let mut encoder = DiffEncoder::new();
        let mut decoder = DiffDecoder::new();
        let key = 500u64;

        let data1: Vec<u8> = (0..4096).map(|i| (i % 256) as u8).collect();
        let mut buf = Vec::new();
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&data1, &mut buf).unwrap();
        let mut cursor = Cursor::new(&buf[..]);
        decoder.decode_blob(&mut cursor).unwrap();

        // Scatter changes AND truncate to 2048
        let mut data2: Vec<u8> = data1[..2048].to_vec();
        for i in (0..data2.len()).step_by(3) {
            data2[i] = data2[i].wrapping_add(7);
        }

        buf.clear();
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&data2, &mut buf).unwrap();

        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data2);
    }

    #[test]
    fn test_diff_empty_blob() {
        let mut encoder = DiffEncoder::new();
        let mut decoder = DiffDecoder::new();
        let key = 600u64;

        // Empty blob
        let data: Vec<u8> = vec![];
        let mut buf = Vec::new();
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&data, &mut buf).unwrap();

        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data);

        // Non-empty second blob after empty first
        let data2 = vec![1u8; 100];
        buf.clear();
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&data2, &mut buf).unwrap();

        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data2);

        // Back to empty
        buf.clear();
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&[], &mut buf).unwrap();

        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_diff_multi_key() {
        let mut encoder = DiffEncoder::new();
        let mut decoder = DiffDecoder::new();

        let key_a = 10u64;
        let key_b = 20u64;

        // First blob for key A
        let data_a1 = vec![0xAAu8; 512];
        let mut buf = Vec::new();
        encoder.set_key(key_a);
        decoder.set_key(key_a);
        encoder.encode_blob(&data_a1, &mut buf).unwrap();
        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data_a1);

        // First blob for key B
        let data_b1 = vec![0xBBu8; 512];
        buf.clear();
        encoder.set_key(key_b);
        decoder.set_key(key_b);
        encoder.encode_blob(&data_b1, &mut buf).unwrap();
        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data_b1);

        // Diff for key A (change 1 byte)
        let mut data_a2 = data_a1.clone();
        data_a2[100] = 0xFF;
        buf.clear();
        encoder.set_key(key_a);
        decoder.set_key(key_a);
        encoder.encode_blob(&data_a2, &mut buf).unwrap();
        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data_a2);

        // Diff for key B (change 1 byte)
        let mut data_b2 = data_b1.clone();
        data_b2[200] = 0xFF;
        buf.clear();
        encoder.set_key(key_b);
        decoder.set_key(key_b);
        encoder.encode_blob(&data_b2, &mut buf).unwrap();
        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data_b2);
    }

    #[test]
    fn test_diff_clear_resets_state() {
        let mut encoder = DiffEncoder::new();
        let mut decoder = DiffDecoder::new();
        let key = 700u64;

        let data1 = vec![0xAAu8; 256];
        let mut buf = Vec::new();
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&data1, &mut buf).unwrap();
        let mut cursor = Cursor::new(&buf[..]);
        decoder.decode_blob(&mut cursor).unwrap();

        // Clear encoder state
        encoder.clear();
        decoder.clear();

        // Next encode for same key should be full blob (no prior state)
        let mut data2 = data1.clone();
        data2[0] = 0xFF;
        buf.clear();
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&data2, &mut buf).unwrap();

        assert_eq!(buf[0], 0, "after clear(), should emit full blob (mode 0)");

        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data2);
    }

    #[test]
    fn test_diff_successive_chain() {
        // Verify a chain of successive diffs roundtrips correctly
        let mut encoder = DiffEncoder::new();
        let mut decoder = DiffDecoder::new();
        let key = 800u64;

        let mut data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();

        // Initial full blob
        let mut buf = Vec::new();
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&data, &mut buf).unwrap();
        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data);

        // 10 successive small mutations
        for i in 0..10 {
            let idx = (i * 100) % data.len();
            data[idx] = (i as u8).wrapping_mul(37);
            buf.clear();
            encoder.set_key(key);
            decoder.set_key(key);
            encoder.encode_blob(&data, &mut buf).unwrap();

            let mut cursor = Cursor::new(&buf[..]);
            let result = decoder.decode_blob(&mut cursor).unwrap();
            assert_eq!(result, data, "mismatch at iteration {i}");
        }
    }

    #[test]
    fn test_diff_invalid_mode_byte() {
        let mut decoder = DiffDecoder::new();
        // Mode byte 3 is invalid
        let mut buf = Vec::new();
        Lencode::encode_varint_u64(3, &mut buf).unwrap();
        let mut cursor = Cursor::new(&buf[..]);
        assert!(decoder.decode_blob(&mut cursor).is_err());
    }

    #[test]
    fn test_diff_u8_array_roundtrip() {
        use crate::context::{DecoderContext, EncoderContext};
        use crate::{Decode, Encode};

        let key = 900u64;
        let mut enc_ctx = EncoderContext {
            dedupe: None,
            diff: Some(DiffEncoder::new()),
        };
        let mut dec_ctx = DecoderContext {
            dedupe: None,
            diff: Some(DiffDecoder::new()),
        };

        // First encode: full blob
        let data1: [u8; 256] = core::array::from_fn(|i| i as u8);
        let mut buf = Vec::new();
        enc_ctx.diff.as_mut().unwrap().set_key(key);
        dec_ctx.diff.as_mut().unwrap().set_key(key);
        data1.encode_ext(&mut buf, Some(&mut enc_ctx)).unwrap();

        let mut cursor = Cursor::new(&buf[..]);
        let result: [u8; 256] = <[u8; 256]>::decode_ext(&mut cursor, Some(&mut dec_ctx)).unwrap();
        assert_eq!(result, data1);

        // Second encode: small diff
        let mut data2 = data1;
        data2[50] = 0xFF;
        data2[200] = 0xFE;
        buf.clear();
        enc_ctx.diff.as_mut().unwrap().set_key(key);
        dec_ctx.diff.as_mut().unwrap().set_key(key);
        data2.encode_ext(&mut buf, Some(&mut enc_ctx)).unwrap();

        assert!(
            buf.len() < 256,
            "diff should be smaller than full array: {} vs 256",
            buf.len()
        );

        let mut cursor = Cursor::new(&buf[..]);
        let result: [u8; 256] = <[u8; 256]>::decode_ext(&mut cursor, Some(&mut dec_ctx)).unwrap();
        assert_eq!(result, data2);
    }

    #[test]
    fn test_diff_u8_slice_encode() {
        use crate::Encode;
        use crate::context::EncoderContext;

        let key = 1000u64;
        let mut enc_ctx = EncoderContext {
            dedupe: None,
            diff: Some(DiffEncoder::new()),
        };

        // First encode: full blob
        let data1: &[u8] = &[0xAA; 512];
        let mut buf = Vec::new();
        enc_ctx.diff.as_mut().unwrap().set_key(key);
        data1.encode_ext(&mut buf, Some(&mut enc_ctx)).unwrap();

        // Second encode: small diff
        let mut data2_vec = vec![0xAA; 512];
        data2_vec[100] = 0xBB;
        let data2: &[u8] = &data2_vec;
        buf.clear();
        enc_ctx.diff.as_mut().unwrap().set_key(key);
        data2.encode_ext(&mut buf, Some(&mut enc_ctx)).unwrap();

        // Mode byte should be 1 (RLE)
        assert_eq!(buf[0], 1, "expected diff mode for &[u8]");
        assert!(buf.len() < 512, "diff should be smaller than full slice");
    }

    #[test]
    fn test_diff_vecdeque_roundtrip() {
        use crate::context::{DecoderContext, EncoderContext};
        use crate::{Decode, Encode};
        #[cfg(not(feature = "std"))]
        use alloc::collections::VecDeque;
        #[cfg(feature = "std")]
        use std::collections::VecDeque;

        let key = 1100u64;
        let mut enc_ctx = EncoderContext {
            dedupe: None,
            diff: Some(DiffEncoder::new()),
        };
        let mut dec_ctx = DecoderContext {
            dedupe: None,
            diff: Some(DiffDecoder::new()),
        };

        // First encode
        let data1: VecDeque<u8> = (0..512).map(|i| (i % 256) as u8).collect();
        let mut buf = Vec::new();
        enc_ctx.diff.as_mut().unwrap().set_key(key);
        dec_ctx.diff.as_mut().unwrap().set_key(key);
        data1.encode_ext(&mut buf, Some(&mut enc_ctx)).unwrap();

        let mut cursor = Cursor::new(&buf[..]);
        let result: VecDeque<u8> = VecDeque::decode_ext(&mut cursor, Some(&mut dec_ctx)).unwrap();
        assert_eq!(result, data1);

        // Second encode: small diff
        let mut data2 = data1.clone();
        data2[50] = 0xFF;
        data2[400] = 0xFE;
        buf.clear();
        enc_ctx.diff.as_mut().unwrap().set_key(key);
        dec_ctx.diff.as_mut().unwrap().set_key(key);
        data2.encode_ext(&mut buf, Some(&mut enc_ctx)).unwrap();

        assert!(
            buf.len() < 512,
            "diff should be smaller: {} vs 512",
            buf.len()
        );

        let mut cursor = Cursor::new(&buf[..]);
        let result: VecDeque<u8> = VecDeque::decode_ext(&mut cursor, Some(&mut dec_ctx)).unwrap();
        assert_eq!(result, data2);
    }

    #[test]
    fn test_diff_encoder_convenience_methods() {
        let mut encoder = DiffEncoder::new();

        assert_eq!(encoder.num_keys(), 0);
        assert_eq!(encoder.cached_bytes(), 0);
        assert!(!encoder.contains_key(1));

        // Store a blob for key 1
        encoder.set_key(1);
        encoder.encode_blob(&[0xAA; 256], &mut Vec::new()).unwrap();

        assert_eq!(encoder.num_keys(), 1);
        assert!(encoder.contains_key(1));
        assert!(!encoder.contains_key(2));
        assert_eq!(encoder.cached_bytes(), 256);

        // Store a blob for key 2
        encoder.set_key(2);
        encoder.encode_blob(&[0xBB; 128], &mut Vec::new()).unwrap();

        assert_eq!(encoder.num_keys(), 2);
        assert_eq!(encoder.cached_bytes(), 384);

        // Remove key 1
        encoder.remove_key(1);
        assert_eq!(encoder.num_keys(), 1);
        assert!(!encoder.contains_key(1));
        assert_eq!(encoder.cached_bytes(), 128);

        // Memory usage should be positive
        assert!(encoder.memory_usage() > 0);
    }

    #[test]
    fn test_diff_decoder_convenience_methods() {
        let mut encoder = DiffEncoder::new();
        let mut decoder = DiffDecoder::new();

        assert_eq!(decoder.num_keys(), 0);
        assert_eq!(decoder.cached_bytes(), 0);

        // Encode + decode a blob
        let key = 10u64;
        encoder.set_key(key);
        decoder.set_key(key);
        let mut buf = Vec::new();
        encoder.encode_blob(&[0xCC; 512], &mut buf).unwrap();
        let mut cursor = Cursor::new(&buf[..]);
        decoder.decode_blob(&mut cursor).unwrap();

        assert_eq!(decoder.num_keys(), 1);
        assert!(decoder.contains_key(key));
        assert_eq!(decoder.cached_bytes(), 512);

        // Remove key
        decoder.remove_key(key);
        assert_eq!(decoder.num_keys(), 0);
        assert!(!decoder.contains_key(key));
        assert_eq!(decoder.cached_bytes(), 0);

        let _usage = decoder.memory_usage();
    }

    #[test]
    fn test_diff_remove_key_forces_full_blob() {
        let mut encoder = DiffEncoder::new();
        let mut decoder = DiffDecoder::new();
        let key = 50u64;

        // First encode (full)
        let data1 = vec![0xAA; 256];
        let mut buf = Vec::new();
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&data1, &mut buf).unwrap();
        let mut cursor = Cursor::new(&buf[..]);
        decoder.decode_blob(&mut cursor).unwrap();

        // Remove key from both
        encoder.remove_key(key);
        decoder.remove_key(key);

        // Next encode should be full blob (mode 0)
        let mut data2 = data1.clone();
        data2[0] = 0xFF;
        buf.clear();
        encoder.set_key(key);
        decoder.set_key(key);
        encoder.encode_blob(&data2, &mut buf).unwrap();

        assert_eq!(
            buf[0], 0,
            "after remove_key(), should emit full blob (mode 0)"
        );

        let mut cursor = Cursor::new(&buf[..]);
        let result = decoder.decode_blob(&mut cursor).unwrap();
        assert_eq!(result, data2);
    }

    #[test]
    fn test_diff_without_key_falls_through() {
        use crate::context::{DecoderContext, EncoderContext};
        use crate::{Decode, Encode};

        // Context with diff but no key set — should use normal encoding
        let mut enc_ctx = EncoderContext {
            dedupe: None,
            diff: Some(DiffEncoder::new()),
        };
        let mut dec_ctx = DecoderContext {
            dedupe: None,
            diff: Some(DiffDecoder::new()),
        };

        let data: Vec<u8> = vec![7u8; 100];
        let mut buf = Vec::new();
        // No key set, so diff is bypassed
        data.encode_ext(&mut buf, Some(&mut enc_ctx)).unwrap();

        let mut cursor = Cursor::new(&buf[..]);
        let result: Vec<u8> = Vec::decode_ext(&mut cursor, Some(&mut dec_ctx)).unwrap();
        assert_eq!(result, data);
    }
}
