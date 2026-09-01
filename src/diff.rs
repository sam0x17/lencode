//! Incremental binary diff encoding/decoding for byte blobs.
//!
//! [`DiffEncoder`] computes compact diffs between successive versions of a keyed
//! byte blob. Its default [`DiffPolicy::Adaptive`] policy uses two strategies
//! and picks whichever is smaller:
//!
//! 1. **RLE patches**: run-length-encoded list of changed regions
//! 2. **XOR + zstd**: XOR the old and new blobs, then zstd-compress the result
//!    (mostly zeros with sparse non-zero bytes compress extremely well)
//!
//! [`DiffDecoder`] reconstructs the full blob from either format. Two decode
//! flavors are offered: [`decode_blob`](DiffDecoder::decode_blob) returns an
//! owned `Vec`, while [`decode_blob_ref`](DiffDecoder::decode_blob_ref)
//! returns a borrow of the reconstruction owned by the decoder's store,
//! the zero-copy path, allocation-free in steady state and preferred on hot
//! paths that copy the result into their own storage anyway.
//!
//! Diff encoding is automatically wired into `Encode`/`Decode` for `Vec<u8>`,
//! `&[u8]`, `[u8; N]`, and `VecDeque<u8>` when an [`EncoderContext`]/[`DecoderContext`]
//! with an active diff key is provided via `encode_ext`/`decode_ext`.
//!
//! ## Wire format
//!
//! Each encoded blob starts with a varint **mode flag**:
//!
//! - `0`: full blob follows (varint length + raw bytes)
//! - `1`: RLE patch diff follows
//! - `2`: XOR + zstd diff follows
//! - `3`: raw XOR diff follows, for streams compressed as a larger outer unit
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
//!
//! Raw XOR format (mode `3`):
//!
//! ```text
//! [new_len: varint]
//! [xor_len: varint]        // exactly max(old_len, new_len)
//! [raw_xor: bytes]         // XOR(old, new), zero-padded if lengths differ
//! ```
//!
//! Modes 0 through 2 are accepted by default. Mode 3 is intended for a
//! versioned containing format with outer compression and requires explicit
//! decoder opt-in through [`DiffDecoder::with_max_supported_mode`].

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use hashbrown::HashMap;

use crate::bytes;
use crate::prelude::*;

#[inline(always)]
fn decode_usize(reader: &mut impl Read) -> Result<usize> {
    usize::try_from(Lencode::decode_varint_u64(reader)?).map_err(|_| Error::DecodeLimitExceeded)
}

/// A single contiguous region of changed bytes.
#[derive(Debug)]
struct Patch<'a> {
    /// Byte offset in the new blob where this patch starts.
    offset: usize,
    /// Changed bytes borrowed from the new blob.
    data: &'a [u8],
}

/// An RLE diff whose wire size is known, but whose frame has not been staged.
struct RleCandidate<'a> {
    patches: Vec<Patch<'a>>,
    encoded_len: usize,
}

/// Minimum gap between patches before they get coalesced into one.
/// Coalescing avoids 2 varint headers (gap + len) when the gap is tiny.
const COALESCE_GAP: usize = 8;
const SCAN_BLOCK: usize = 32;

/// Highest diff mode accepted by default constructors and skip helpers.
pub const DEFAULT_MAX_DIFF_MODE: u8 = 2;

/// Raw XOR mode used by formats that compress a larger containing unit.
pub const RAW_XOR_DIFF_MODE: u8 = 3;

#[inline(always)]
const fn varint_len(value: usize) -> usize {
    if value <= 0x7f {
        1
    } else {
        1 + ((usize::BITS - value.leading_zeros() + 7) >> 3) as usize
    }
}

/// Computes patches between `old` and `new` byte slices.
///
/// Adjacent patches separated by fewer than [`COALESCE_GAP`] bytes are merged.
/// Returns `None` if the patch data would exceed half the new blob size (full
/// blob is more compact in that case).
#[inline(never)]
fn compute_patches<'a>(old: &[u8], new: &'a [u8]) -> Option<Vec<Patch<'a>>> {
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
                data: &new[start..i],
            });
        } else {
            i += 1;
        }
    }

    // If new is longer, the tail is a patch
    if new.len() > old.len() {
        patches.push(Patch {
            offset: old.len(),
            data: &new[old.len()..],
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
                // Merge by widening the borrowed range to cover the gap.
                let p_end = p.offset + p.data.len();
                last.data = &new[last.offset..p_end];
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
        // For zero or one patch, check size.
        let patch_bytes: usize = patches.iter().map(|p| p.data.len()).sum();
        if patch_bytes > new.len() / 2 {
            return None;
        }
        Some(patches)
    }
}

#[inline(always)]
fn should_scan_blocks(old: &[u8], new: &[u8]) -> bool {
    let min_len = old.len().min(new.len());
    if min_len < SCAN_BLOCK * 2 {
        return false;
    }

    old[..SCAN_BLOCK] == new[..SCAN_BLOCK]
        && old[min_len - SCAN_BLOCK..min_len] == new[min_len - SCAN_BLOCK..min_len]
}

/// Computes sparse patches while skipping equal blocks wholesale. This stays
/// separate so dense inputs retain the compact bytewise scanner's code layout.
#[inline(never)]
fn compute_patches_blockwise<'a>(old: &[u8], new: &'a [u8]) -> Option<Vec<Patch<'a>>> {
    let min_len = old.len().min(new.len());
    let mut patches: Vec<Patch> = Vec::new();
    let mut i = 0;

    while i < min_len {
        let remaining = min_len - i;
        let block_end = if remaining >= SCAN_BLOCK {
            i + SCAN_BLOCK
        } else {
            min_len
        };
        if old[i..block_end] == new[i..block_end] {
            i = block_end;
            continue;
        }

        while i < block_end {
            if old[i] != new[i] {
                let start = i;
                // Runs crossing a block boundary are joined below.
                while i < block_end && old[i] != new[i] {
                    i += 1;
                }
                patches.push(Patch {
                    offset: start,
                    data: &new[start..i],
                });
            } else {
                i += 1;
            }
        }
    }

    if new.len() > old.len() {
        patches.push(Patch {
            offset: old.len(),
            data: &new[old.len()..],
        });
    }

    if patches.len() > 1 {
        let mut coalesced: Vec<Patch> = Vec::with_capacity(patches.len());
        coalesced.push(patches.remove(0));
        for p in patches {
            let last = coalesced.last_mut().unwrap();
            let last_end = last.offset + last.data.len();
            let gap = p.offset - last_end;
            if gap < COALESCE_GAP {
                let p_end = p.offset + p.data.len();
                last.data = &new[last.offset..p_end];
            } else {
                coalesced.push(p);
            }
        }
        let patch_bytes: usize = coalesced.iter().map(|p| p.data.len()).sum();
        if patch_bytes > new.len() / 2 {
            return None;
        }
        Some(coalesced)
    } else {
        let patch_bytes: usize = patches.iter().map(|p| p.data.len()).sum();
        if patch_bytes > new.len() / 2 {
            return None;
        }
        Some(patches)
    }
}

/// Plan an RLE frame without copying its patch payloads into a staging buffer.
fn plan_rle<'a>(old: &[u8], new: &'a [u8]) -> Option<RleCandidate<'a>> {
    let patches = if should_scan_blocks(old, new) {
        compute_patches_blockwise(old, new)?
    } else {
        compute_patches(old, new)?
    };
    let mut encoded_len = 1 + varint_len(new.len()) + varint_len(patches.len());
    let mut cursor = 0usize;
    for patch in &patches {
        encoded_len +=
            varint_len(patch.offset - cursor) + varint_len(patch.data.len()) + patch.data.len();
        cursor = patch.offset + patch.data.len();
    }
    Some(RleCandidate {
        patches,
        encoded_len,
    })
}

/// Materialize a planned RLE frame after it has won candidate selection.
fn encode_rle_candidate(candidate: &RleCandidate<'_>, new_len: usize) -> Option<Vec<u8>> {
    // VecWriter's direct varint path requires 17 spare bytes. Keep that
    // slack so the exact-size staging buffer never has to grow at the end.
    let mut buf = VecWriter::with_capacity(candidate.encoded_len.checked_add(16)?);
    // Mode 1 = RLE patches
    Lencode::encode_varint_u64(1, &mut buf).ok()?;
    Lencode::encode_varint_u64(new_len as u64, &mut buf).ok()?;
    Lencode::encode_varint_u64(candidate.patches.len() as u64, &mut buf).ok()?;

    let mut cursor = 0usize;
    for patch in &candidate.patches {
        let gap = patch.offset - cursor;
        Lencode::encode_varint_u64(gap as u64, &mut buf).ok()?;
        Lencode::encode_varint_u64(patch.data.len() as u64, &mut buf).ok()?;
        buf.write(patch.data).ok()?;
        cursor = patch.offset + patch.data.len();
    }
    Some(buf.into_inner())
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

/// Controls which diff representation [`DiffEncoder`] emits.
///
/// Changing policy does not reset keyed history. This permits safe transitions
/// between policies while an encoder and decoder remain in sync.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DiffPolicy {
    /// Preserve the original behavior: choose between RLE patches, XOR + zstd,
    /// and a full frame using the existing heuristics.
    #[default]
    Adaptive,
    /// Try RLE patches only, falling back to a full frame when the changes are
    /// too dense for the existing RLE candidate threshold.
    RleOnly,
    /// Choose the smaller exact frame between RLE patches and raw XOR for each
    /// update with prior keyed state, leaving any remaining compression to the
    /// containing stream or file.
    ///
    /// This is an experimental policy. Its effectiveness depends on outer
    /// frame boundaries and must be measured on representative workloads.
    OuterCompressed,
    /// Emit full frames only. Keyed state is still retained so changing policy
    /// later cannot desynchronize the paired decoder.
    Disabled,
}

/// Writes a mode-3 raw XOR frame without allocating an XOR-sized buffer.
fn encode_raw_xor(old: &[u8], new: &[u8], writer: &mut impl Write) -> Result<usize> {
    let xor_len = old.len().max(new.len());
    let overlap_len = old.len().min(new.len());
    let mut total = 0usize;
    total += Lencode::encode_varint_u64(u64::from(RAW_XOR_DIFF_MODE), writer)?;
    total += Lencode::encode_varint_u64(new.len() as u64, writer)?;
    total += Lencode::encode_varint_u64(xor_len as u64, writer)?;

    let mut offset = 0usize;
    let mut scratch = [0u8; 4096];
    while offset < overlap_len {
        let chunk_len = (overlap_len - offset).min(scratch.len());
        for (dest, (&old_byte, &new_byte)) in scratch[..chunk_len]
            .iter_mut()
            .zip(old[offset..].iter().zip(&new[offset..]))
        {
            *dest = old_byte ^ new_byte;
        }
        total += writer.write(&scratch[..chunk_len])?;
        offset += chunk_len;
    }

    let tail = if new.len() > overlap_len {
        &new[overlap_len..]
    } else {
        &old[overlap_len..]
    };
    if !tail.is_empty() {
        total += writer.write(tail)?;
    }
    Ok(total)
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
    /// Candidate-selection policy. Defaults to the original adaptive behavior.
    policy: DiffPolicy,
    /// Maximum retained capacity across all keyed blobs.
    max_cache_bytes: usize,
    /// Maximum number of keyed blobs retained at once.
    max_cache_keys: usize,
    /// Sum of the capacities of every Vec in `store`.
    cache_capacity_bytes: usize,
    /// Per-reset high-water marks for observability and limit sizing.
    peak_cache_capacity_bytes: usize,
    peak_cache_keys: usize,
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
        Self::with_policy(DiffPolicy::Adaptive)
    }

    /// Creates a new `DiffEncoder` with pre-allocated capacity for `num_keys` keys.
    #[inline(always)]
    pub fn with_capacity(num_keys: usize) -> Self {
        Self::with_capacity_and_policy(num_keys, DiffPolicy::Adaptive)
    }

    /// Creates an empty encoder with an explicit diff policy.
    #[inline(always)]
    pub fn with_policy(policy: DiffPolicy) -> Self {
        Self::with_capacity_and_policy(0, policy)
    }

    /// Creates an encoder with pre-allocated key capacity and an explicit
    /// diff policy.
    #[inline(always)]
    pub fn with_capacity_and_policy(num_keys: usize, policy: DiffPolicy) -> Self {
        Self::with_capacity_policy_and_cache_limits(num_keys, policy, usize::MAX, usize::MAX)
    }

    /// Creates an encoder with explicit bounds on retained keyed state.
    ///
    /// `max_cache_bytes` charges retained `Vec` capacity, rather than current
    /// blob length, so shrinking many values cannot leave unbounded resident
    /// allocations behind. `max_cache_keys` separately bounds the hash table;
    /// in particular, zero-length blobs cannot bypass the byte limit.
    #[inline(always)]
    pub fn with_capacity_policy_and_cache_limits(
        num_keys: usize,
        policy: DiffPolicy,
        max_cache_bytes: usize,
        max_cache_keys: usize,
    ) -> Self {
        Self {
            store: HashMap::with_capacity(num_keys.min(max_cache_keys)),
            current_key: None,
            policy,
            max_cache_bytes,
            max_cache_keys,
            cache_capacity_bytes: 0,
            peak_cache_capacity_bytes: 0,
            peak_cache_keys: 0,
        }
    }

    /// Returns the active diff policy.
    #[inline(always)]
    pub const fn policy(&self) -> DiffPolicy {
        self.policy
    }

    /// Selects the policy used for subsequent frames without discarding keyed
    /// history.
    #[inline(always)]
    pub const fn set_policy(&mut self, policy: DiffPolicy) {
        self.policy = policy;
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
        self.cache_capacity_bytes = 0;
        self.peak_cache_capacity_bytes = 0;
        self.peak_cache_keys = 0;
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
        if let Some(removed) = self.store.remove(&key) {
            self.cache_capacity_bytes -= removed.capacity();
        }
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

    /// Retained keyed-blob capacity charged against the configured limit.
    #[inline(always)]
    pub const fn cache_capacity_bytes(&self) -> usize {
        self.cache_capacity_bytes
    }

    /// Highest retained keyed-blob capacity since construction or `clear`.
    #[inline(always)]
    pub const fn peak_cache_capacity_bytes(&self) -> usize {
        self.peak_cache_capacity_bytes
    }

    /// Highest keyed-blob count since construction or `clear`.
    #[inline(always)]
    pub const fn peak_cache_keys(&self) -> usize {
        self.peak_cache_keys
    }

    /// Encodes a byte blob, producing a diff against the previously seen value
    /// for the current key (if any).
    ///
    /// Candidate selection is controlled by [`DiffPolicy`]. The default
    /// [`DiffPolicy::Adaptive`] behavior is wire-compatible with earlier
    /// releases.
    ///
    /// Returns the number of bytes written.
    pub fn encode_blob(&mut self, data: &[u8], writer: &mut impl Write) -> Result<usize> {
        if let Some(key) = self.current_key
            && let Some(old) = self.store.get(&key)
        {
            if self.policy == DiffPolicy::OuterCompressed {
                let raw_xor_len = old.len().max(data.len());
                let raw_frame_len = 1usize
                    .checked_add(varint_len(data.len()))
                    .and_then(|len| len.checked_add(varint_len(raw_xor_len)))
                    .and_then(|len| len.checked_add(raw_xor_len))
                    .unwrap_or(usize::MAX);
                if let Some(rle) = plan_rle(old, data)
                    && rle.encoded_len <= raw_frame_len
                    && let Some(buf) = encode_rle_candidate(&rle, data.len())
                {
                    let prepared = self.prepare_remember(key, data)?;
                    let written = writer.write(&buf)?;
                    self.commit_remember(key, data, prepared);
                    return Ok(written);
                }

                let prepared = self.prepare_remember(key, data)?;
                let old = self
                    .store
                    .get(&key)
                    .expect("existing keyed value cannot disappear during preparation");
                let written = encode_raw_xor(old, data, writer)?;
                self.commit_remember(key, data, prepared);
                return Ok(written);
            }

            if self.policy != DiffPolicy::Disabled {
                // Plan RLE first without staging its payload. If XOR wins,
                // the discarded RLE frame never copies patch bytes.
                let rle_candidate = plan_rle(old, data);
                let winner = if self.policy == DiffPolicy::RleOnly {
                    rle_candidate
                        .as_ref()
                        .and_then(|rle| encode_rle_candidate(rle, data.len()))
                } else {
                    // Skip expensive XOR+zstd when RLE is already compact
                    // (less than 10% of the blob), preserving the original
                    // adaptive heuristic exactly.
                    let rle_is_tiny = rle_candidate
                        .as_ref()
                        .is_some_and(|candidate| candidate.encoded_len * 10 <= data.len());
                    let xor_candidate = if rle_is_tiny {
                        None
                    } else {
                        self.encode_xor_to_buf(old, data)
                    };

                    match (rle_candidate, xor_candidate) {
                        (Some(rle), Some(xor)) => {
                            if rle.encoded_len <= xor.len() {
                                encode_rle_candidate(&rle, data.len())
                            } else {
                                Some(xor)
                            }
                        }
                        (Some(rle), None) => encode_rle_candidate(&rle, data.len()),
                        (None, Some(xor)) => Some(xor),
                        (None, None) => None,
                    }
                };

                if let Some(buf) = winner {
                    let prepared = self.prepare_remember(key, data)?;
                    let written = writer.write(&buf)?;
                    self.commit_remember(key, data, prepared);
                    return Ok(written);
                }
            }
        }

        // No previous value, disabled diffs, or no winning candidate: write a
        // full blob. Update keyed state only after all writes succeed.
        let prepared = if let Some(key) = self.current_key {
            Some((key, self.prepare_remember(key, data)?))
        } else {
            None
        };
        let mut total = 0;
        total += Lencode::encode_varint_u64(0, writer)?;
        total += Lencode::encode_varint_u64(data.len() as u64, writer)?;
        total += writer.write(data)?;
        if let Some((key, replacement)) = prepared {
            self.commit_remember(key, data, replacement);
        }
        Ok(total)
    }

    /// Prepares a cache update before any output is emitted. Capacity-only
    /// changes are semantically inert, so a writer failure leaves keyed
    /// history exactly as it was before the call.
    fn prepare_remember(&mut self, key: u64, data: &[u8]) -> Result<Option<Vec<u8>>> {
        let old_capacity = self.store.get(&key).map_or(0, Vec::capacity);
        if self.store.contains_key(&key) && old_capacity >= data.len() {
            return Ok(None);
        }
        if !self.store.contains_key(&key) {
            if self.store.len() >= self.max_cache_keys {
                return Err(Error::WriterOutOfSpace);
            }
            self.store
                .try_reserve(1)
                .map_err(|_| Error::WriterOutOfSpace)?;
        }

        let base = self
            .cache_capacity_bytes
            .checked_sub(old_capacity)
            .ok_or(Error::WriterOutOfSpace)?;
        let available = self
            .max_cache_bytes
            .checked_sub(base)
            .ok_or(Error::WriterOutOfSpace)?;
        if data.len() > available {
            return Err(Error::WriterOutOfSpace);
        }
        let mut replacement = Vec::new();
        replacement
            .try_reserve_exact(data.len())
            .map_err(|_| Error::WriterOutOfSpace)?;
        let new_total = base
            .checked_add(replacement.capacity())
            .ok_or(Error::WriterOutOfSpace)?;
        if new_total > self.max_cache_bytes {
            return Err(Error::WriterOutOfSpace);
        }
        replacement.extend_from_slice(data);
        Ok(Some(replacement))
    }

    /// Commits a cache update after the complete frame reached the writer.
    #[inline]
    fn commit_remember(&mut self, key: u64, data: &[u8], replacement: Option<Vec<u8>>) {
        if let Some(replacement) = replacement {
            let old_capacity = self.store.get(&key).map_or(0, Vec::capacity);
            self.cache_capacity_bytes =
                self.cache_capacity_bytes - old_capacity + replacement.capacity();
            self.store.insert(key, replacement);
        } else {
            let stored = self
                .store
                .get_mut(&key)
                .expect("prepared reusable cache entry must exist");
            stored.clear();
            stored.extend_from_slice(data);
        }
        self.peak_cache_capacity_bytes = self
            .peak_cache_capacity_bytes
            .max(self.cache_capacity_bytes);
        self.peak_cache_keys = self.peak_cache_keys.max(self.store.len());
    }

    /// Encode RLE patches into a temporary buffer. Returns `None` if patches
    /// are too large (would exceed half the blob size).
    pub fn encode_rle_to_buf(&self, old: &[u8], new: &[u8]) -> Option<Vec<u8>> {
        let candidate = plan_rle(old, new)?;
        encode_rle_candidate(&candidate, new.len())
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

/// Advances past one complete diff-blob frame without reconstructing it.
///
/// This validates structural framing for the default modes: full blobs (0),
/// RLE patches (1), and XOR + zstd (2). RLE patch ranges are checked, but
/// payloads are not reconstructed or decompressed. Stateful relationships,
/// such as whether a diff frame has a prior blob under its key, require normal
/// decoding and are intentionally outside this function.
///
/// Readers that expose [`Read::buf`] advance without copying. Other readers
/// use a small fixed-size discard buffer.
pub fn skip_diff_blob_frame(reader: &mut impl Read) -> Result<()> {
    skip_diff_blob_frame_with_max_mode(reader, DEFAULT_MAX_DIFF_MODE)
}

/// Advances past one diff-blob frame only when its mode is no greater than
/// `max_mode`.
///
/// This lets a containing versioned format reject newer diff modes before
/// consuming their headers or payloads. Use `2` for formats that predate raw
/// XOR mode and [`RAW_XOR_DIFF_MODE`] for all modes currently defined by this
/// crate.
pub fn skip_diff_blob_frame_with_max_mode(reader: &mut impl Read, max_mode: u8) -> Result<()> {
    skip_diff_blob_frame_with_max_mode_and_len(reader, max_mode).map(|_| ())
}

/// Advances past one diff-blob frame and returns its declared reconstructed
/// length without materializing the blob.
///
/// This is useful to enforce semantic data-volume limits even when a caller
/// intentionally skips account bytes. It performs the same structural framing
/// and resource-bound checks as [`skip_diff_blob_frame_with_max_mode`]. Full
/// canonical validation requires normal stateful decoding.
pub fn skip_diff_blob_frame_with_max_mode_and_len(
    reader: &mut impl Read,
    max_mode: u8,
) -> Result<usize> {
    let mode = Lencode::decode_varint_u64(reader)?;
    if mode > u64::from(max_mode) {
        return Err(Error::InvalidData);
    }

    match mode {
        0 => {
            let len = decode_usize(reader)?;
            reader.claim_blob(len)?;
            skip_reader_bytes(reader, len)?;
            Ok(len)
        }
        1 => {
            let new_len = decode_usize(reader)?;
            let num_patches = decode_usize(reader)?;
            reader.claim_blob(new_len)?;
            reader.claim_sequence(num_patches, 1)?;

            let mut previous_end = 0usize;
            for _ in 0..num_patches {
                let gap = decode_usize(reader)?;
                let patch_len = decode_usize(reader)?;
                let patch_start = previous_end.checked_add(gap).ok_or(Error::InvalidData)?;
                let patch_end = patch_start
                    .checked_add(patch_len)
                    .filter(|end| *end <= new_len)
                    .ok_or(Error::InvalidData)?;
                skip_reader_bytes(reader, patch_len)?;
                previous_end = patch_end;
            }
            Ok(new_len)
        }
        2 => {
            let new_len = decode_usize(reader)?;
            let compressed_len = decode_usize(reader)?;
            reader.claim_blob(new_len)?;
            skip_reader_bytes(reader, compressed_len)?;
            Ok(new_len)
        }
        3 => {
            let new_len = decode_usize(reader)?;
            let xor_len = decode_usize(reader)?;
            if xor_len < new_len {
                return Err(Error::InvalidData);
            }
            reader.claim_blob(xor_len)?;
            skip_reader_bytes(reader, xor_len)?;
            Ok(new_len)
        }
        _ => Err(Error::InvalidData),
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
    /// Reused assembly buffer for [`decode_blob_ref`](Self::decode_blob_ref):
    /// diff results are built here, then swapped into the store slot so both
    /// buffers' capacities recycle across calls.
    result_scratch: Vec<u8>,
    /// Reused staging for mode-2 compressed bytes (borrowed path).
    compressed_scratch: Vec<u8>,
    /// Reused staging for mode-2 decompressed XOR bytes (borrowed path).
    xor_scratch: Vec<u8>,
    /// Highest wire mode accepted by this decoder.
    max_supported_mode: u8,
    /// Maximum retained capacity across all keyed blobs.
    max_cache_bytes: usize,
    /// Maximum number of keyed blobs retained at once.
    max_cache_keys: usize,
    /// Sum of the capacities of every Vec in `store`.
    cache_capacity_bytes: usize,
    /// Per-reset high-water marks for observability and limit sizing.
    peak_cache_capacity_bytes: usize,
    peak_cache_keys: usize,
}

impl Default for DiffDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl DiffDecoder {
    /// Creates a new empty decoder that accepts diff modes 0 through 2.
    ///
    /// Raw XOR mode 3 requires an explicit containing-format version and
    /// [`with_max_supported_mode`](Self::with_max_supported_mode).
    #[inline(always)]
    pub fn new() -> Self {
        Self::with_max_supported_mode(DEFAULT_MAX_DIFF_MODE)
    }

    /// Creates a decoder with pre-allocated capacity that accepts modes 0
    /// through 2.
    #[inline(always)]
    pub fn with_capacity(num_keys: usize) -> Self {
        Self::with_capacity_and_max_mode(num_keys, DEFAULT_MAX_DIFF_MODE)
    }

    /// Creates an empty decoder that rejects modes greater than `max_mode`.
    ///
    /// Use [`DEFAULT_MAX_DIFF_MODE`] for a format that predates raw XOR mode,
    /// or [`RAW_XOR_DIFF_MODE`] for a versioned format that defines mode 3.
    /// Values above [`RAW_XOR_DIFF_MODE`] do not make undefined modes valid.
    #[inline(always)]
    pub fn with_max_supported_mode(max_mode: u8) -> Self {
        Self::with_capacity_and_max_mode(0, max_mode)
    }

    /// Creates a decoder with pre-allocated key capacity and an explicit
    /// maximum accepted wire mode.
    #[inline(always)]
    pub fn with_capacity_and_max_mode(num_keys: usize, max_mode: u8) -> Self {
        Self::with_capacity_mode_and_cache_limits(num_keys, max_mode, usize::MAX, usize::MAX)
    }

    /// Creates a decoder with explicit bounds on retained keyed state.
    ///
    /// The byte limit charges retained `Vec` capacity, not only current blob
    /// length. The key limit independently bounds hash-table growth and makes
    /// zero-length keyed blobs consume a finite resource.
    #[inline(always)]
    pub fn with_capacity_mode_and_cache_limits(
        num_keys: usize,
        max_mode: u8,
        max_cache_bytes: usize,
        max_cache_keys: usize,
    ) -> Self {
        Self {
            store: HashMap::with_capacity(num_keys.min(max_cache_keys)),
            current_key: None,
            result_scratch: Vec::new(),
            compressed_scratch: Vec::new(),
            xor_scratch: Vec::new(),
            max_supported_mode: max_mode,
            max_cache_bytes,
            max_cache_keys,
            cache_capacity_bytes: 0,
            peak_cache_capacity_bytes: 0,
            peak_cache_keys: 0,
        }
    }

    /// Returns the highest wire mode this decoder accepts.
    #[inline(always)]
    pub const fn max_supported_mode(&self) -> u8 {
        self.max_supported_mode
    }

    /// Changes the highest accepted wire mode without modifying keyed state.
    /// Values above [`RAW_XOR_DIFF_MODE`] do not make undefined modes valid.
    #[inline(always)]
    pub const fn set_max_supported_mode(&mut self, max_mode: u8) {
        self.max_supported_mode = max_mode;
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
        self.cache_capacity_bytes = 0;
        self.peak_cache_capacity_bytes = 0;
        self.peak_cache_keys = 0;
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
        if let Some(removed) = self.store.remove(&key) {
            self.cache_capacity_bytes -= removed.capacity();
        }
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

    /// Retained keyed-blob capacity charged against the configured limit.
    #[inline(always)]
    pub const fn cache_capacity_bytes(&self) -> usize {
        self.cache_capacity_bytes
    }

    /// Highest retained keyed-blob capacity since construction or `clear`.
    #[inline(always)]
    pub const fn peak_cache_capacity_bytes(&self) -> usize {
        self.peak_cache_capacity_bytes
    }

    /// Highest keyed-blob count since construction or `clear`.
    #[inline(always)]
    pub const fn peak_cache_keys(&self) -> usize {
        self.peak_cache_keys
    }

    /// Decodes a byte blob, applying patches if the stream contains a diff.
    ///
    /// Returns the reconstructed blob as an owned `Vec`. This is the
    /// convenience wrapper over [`decode_blob_ref`](Self::decode_blob_ref);
    /// callers on a hot path should prefer the borrowed variant, which
    /// performs no allocation in steady state.
    pub fn decode_blob(&mut self, reader: &mut impl Read) -> Result<Vec<u8>> {
        let decoded = self.decode_blob_ref(reader)?;
        reader.claim_blob(decoded.len())?;
        Ok(decoded.to_vec())
    }

    /// Decodes a byte blob like [`decode_blob`](Self::decode_blob), but
    /// returns a borrow of the reconstruction owned by the decoder. The
    /// zero-copy path.
    ///
    /// The blob lands in the store under the active key (the previous
    /// value's buffer is recycled as the next assembly scratch), so once
    /// buffers have grown to blob size, decoding allocates nothing. With no
    /// active key the reconstruction lives in an internal scratch until the
    /// next call. The borrow ends at the next decode; copy out (e.g. into
    /// an arena) anything you keep.
    ///
    /// ```
    /// use lencode::diff::{DiffDecoder, DiffEncoder};
    /// use lencode::io::Cursor;
    ///
    /// let v1 = vec![7u8; 256];
    /// let mut v2 = v1.clone();
    /// v2[9] = 0; // The second segment encodes as a diff.
    ///
    /// let mut encoder = DiffEncoder::new();
    /// let mut buf = Vec::new();
    /// encoder.set_key(42);
    /// encoder.encode_blob(&v1, &mut buf).unwrap();
    /// encoder.set_key(42);
    /// encoder.encode_blob(&v2, &mut buf).unwrap();
    ///
    /// let mut decoder = DiffDecoder::new();
    /// let mut cursor = Cursor::new(&buf[..]);
    /// decoder.set_key(42);
    /// assert_eq!(decoder.decode_blob_ref(&mut cursor).unwrap(), &v1[..]);
    /// decoder.set_key(42);
    /// assert_eq!(decoder.decode_blob_ref(&mut cursor).unwrap(), &v2[..]);
    /// ```
    pub fn decode_blob_ref(&mut self, reader: &mut impl Read) -> Result<&[u8]> {
        let mode = Lencode::decode_varint_u64(reader)?;
        if mode > u64::from(self.max_supported_mode) {
            return Err(Error::InvalidData);
        }

        match mode {
            0 => {
                // Full blob. Assemble before replacing cached state so a
                // truncated or adversarial reader cannot discard the prior
                // value for this key.
                let len = decode_usize(reader)?;
                reader.claim_blob(len)?;
                match self.current_key {
                    Some(key) => {
                        self.prepare_keyed_result(key, len)?;
                        read_exact_into(reader, len, &mut self.result_scratch)?;
                        self.commit_keyed_result(key);
                        let slot = self
                            .store
                            .get(&key)
                            .expect("prepared cache entry must exist");
                        Ok(slot.as_slice())
                    }
                    None => {
                        try_prepare_scratch(&mut self.result_scratch, len)?;
                        read_exact_into(reader, len, &mut self.result_scratch)?;
                        Ok(self.result_scratch.as_slice())
                    }
                }
            }
            1 => {
                // Patch diffs require the old blob.
                let new_len = decode_usize(reader)?;
                let num_patches = decode_usize(reader)?;
                reader.claim_blob(new_len)?;
                reader.claim_sequence(num_patches, 1)?;

                let key = self.current_key.ok_or(Error::InvalidData)?;
                let old_len = self.store.get(&key).ok_or(Error::InvalidData)?.len();

                // Same-length RLE frames can update the cached blob directly
                // when the reader exposes its remaining bytes. Validate the
                // complete patch stream first so malformed input never leaves
                // the cache partially updated, then copy only changed bytes.
                if new_len == old_len {
                    if num_patches == 0 {
                        return Ok(self
                            .store
                            .get(&key)
                            .expect("old blob was checked above")
                            .as_slice());
                    }
                    if let Some(input) = reader.buf() {
                        let consumed = visit_rle_patches(input, new_len, num_patches, |_, _| {})?;
                        let slot = self
                            .store
                            .get_mut(&key)
                            .expect("old blob was checked above");
                        visit_rle_patches(
                            &input[..consumed],
                            new_len,
                            num_patches,
                            |offset, patch| {
                                slot[offset..offset + patch.len()].copy_from_slice(patch);
                            },
                        )?;
                        reader.advance(consumed);
                        return Ok(slot.as_slice());
                    }
                }

                // Assemble into the scratch while borrowing `old` from the
                // store (disjoint fields), then swap the finished blob into
                // the store slot below.
                self.prepare_keyed_result(key, new_len)?;
                {
                    let old = self.store.get(&key).ok_or(Error::InvalidData)?;
                    let result = &mut self.result_scratch;
                    result.clear();
                    result.reserve(new_len);
                    let mut old_cursor = 0usize;

                    for _ in 0..num_patches {
                        let gap = decode_usize(reader)?;
                        let patch_len = decode_usize(reader)?;

                        // Copy unchanged bytes from old blob
                        let copy_end = old_cursor
                            .checked_add(gap)
                            .filter(|end| *end <= old.len())
                            .ok_or(Error::InvalidData)?;
                        let patch_end = copy_end
                            .checked_add(patch_len)
                            .filter(|end| *end <= new_len)
                            .ok_or(Error::InvalidData)?;
                        result.extend_from_slice(&old[old_cursor..copy_end]);

                        // Initialize the destination before lending it to an
                        // arbitrary reader. Cached state is replaced only after
                        // the entire patch stream validates.
                        let start = result.len();
                        debug_assert_eq!(start, copy_end);
                        result.resize(patch_end, 0);
                        if let Err(error) =
                            crate::io::read_exact_bytes(reader, &mut result[start..patch_end])
                        {
                            result.clear();
                            return Err(error);
                        }

                        old_cursor = patch_end;
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
                }
                self.commit_keyed_result(key);
                let slot = self.store.get(&key).ok_or(Error::InvalidData)?;
                Ok(slot.as_slice())
            }
            2 => {
                // XOR + zstd diff
                let new_len = decode_usize(reader)?;
                let compressed_len = decode_usize(reader)?;
                let key = self.current_key.ok_or(Error::InvalidData)?;
                self.decode_xor_ref(reader, key, new_len, compressed_len)
            }
            3 => {
                // Raw XOR diff intended for an outer compressed stream.
                let new_len = decode_usize(reader)?;
                let xor_len = decode_usize(reader)?;
                let key = self.current_key.ok_or(Error::InvalidData)?;
                self.decode_raw_xor_ref(reader, key, new_len, xor_len)
            }
            _ => Err(Error::InvalidData),
        }
    }

    fn decode_raw_xor_ref(
        &mut self,
        reader: &mut impl Read,
        key: u64,
        new_len: usize,
        xor_len: usize,
    ) -> Result<&[u8]> {
        let old_len = self.store.get(&key).ok_or(Error::InvalidData)?.len();
        if xor_len != old_len.max(new_len) {
            return Err(Error::InvalidData);
        }
        reader.claim_blob(xor_len)?;

        if new_len != old_len {
            self.prepare_keyed_result(key, new_len)?;
        }

        if let Some(input) = reader.buf() {
            if input.len() < xor_len {
                return Err(Error::ReaderOutOfData);
            }
            let xor = &input[..xor_len];
            validate_raw_xor_tail(
                self.store.get(&key).expect("old blob was checked above"),
                new_len,
                xor,
            )?;

            if new_len == old_len {
                let slot = self
                    .store
                    .get_mut(&key)
                    .expect("old blob was checked above");
                for (byte, delta) in slot.iter_mut().zip(xor) {
                    *byte ^= *delta;
                }
                reader.advance(xor_len);
                return Ok(slot.as_slice());
            }

            reconstruct_raw_xor(
                self.store.get(&key).expect("old blob was checked above"),
                new_len,
                xor,
                &mut self.result_scratch,
            );
            reader.advance(xor_len);
        } else {
            try_prepare_scratch(&mut self.xor_scratch, xor_len)?;
            read_exact_into(reader, xor_len, &mut self.xor_scratch)?;
            validate_raw_xor_tail(
                self.store.get(&key).expect("old blob was checked above"),
                new_len,
                &self.xor_scratch,
            )?;

            if new_len == old_len {
                let xor = &self.xor_scratch;
                let slot = self
                    .store
                    .get_mut(&key)
                    .expect("old blob was checked above");
                for (byte, delta) in slot.iter_mut().zip(xor) {
                    *byte ^= *delta;
                }
                return Ok(slot.as_slice());
            }

            reconstruct_raw_xor(
                self.store.get(&key).expect("old blob was checked above"),
                new_len,
                &self.xor_scratch,
                &mut self.result_scratch,
            );
        }

        self.commit_keyed_result(key);
        let slot = self.store.get(&key).ok_or(Error::InvalidData)?;
        Ok(slot.as_slice())
    }

    #[inline(never)]
    fn decode_xor_ref(
        &mut self,
        reader: &mut impl Read,
        key: u64,
        new_len: usize,
        compressed_len: usize,
    ) -> Result<&[u8]> {
        let old_len = self.store.get(&key).ok_or(Error::InvalidData)?.len();
        let xor_len = old_len.max(new_len);
        reader.claim_blob(xor_len)?;

        try_prepare_scratch(&mut self.xor_scratch, xor_len)?;
        if new_len != old_len {
            self.prepare_keyed_result(key, new_len)?;
        }

        // Borrow contiguous compressed input when possible; generic readers
        // retain the reusable staging-buffer fallback.
        if let Some(input) = reader.buf() {
            if input.len() < compressed_len {
                return Err(Error::ReaderOutOfData);
            }
            bytes::zstd_decompress_into(&input[..compressed_len], xor_len, &mut self.xor_scratch)?;
            reader.advance(compressed_len);
        } else {
            reader.claim_allocation(compressed_len)?;
            try_prepare_scratch(&mut self.compressed_scratch, compressed_len)?;
            read_exact_into(reader, compressed_len, &mut self.compressed_scratch)?;
            bytes::zstd_decompress_into(&self.compressed_scratch, xor_len, &mut self.xor_scratch)?;
        }

        validate_raw_xor_tail(
            self.store.get(&key).expect("old blob was checked above"),
            new_len,
            &self.xor_scratch,
        )?;

        // Once decompression succeeds, a same-length update cannot fail.
        // Apply it directly to the cached allocation instead of reconstructing
        // and swapping a second full-size buffer.
        if new_len == old_len {
            let xor = &self.xor_scratch;
            let slot = self
                .store
                .get_mut(&key)
                .expect("old blob was checked above");
            for (byte, delta) in slot.iter_mut().zip(xor) {
                *byte ^= *delta;
            }
            return Ok(slot.as_slice());
        }

        // Length-changing diffs retain the reconstruct-and-swap path.
        {
            let old = self.store.get(&key).ok_or(Error::InvalidData)?;
            let xor = &self.xor_scratch;
            let result = &mut self.result_scratch;
            result.clear();
            result.reserve(new_len);
            let min_len = old.len().min(new_len);
            for i in 0..min_len {
                result.push(old[i] ^ xor[i]);
            }
            if new_len > old.len() {
                result.extend_from_slice(&xor[min_len..new_len]);
            }

            if result.len() != new_len {
                return Err(Error::InvalidData);
            }
        }
        self.commit_keyed_result(key);
        let slot = self.store.get(&key).ok_or(Error::InvalidData)?;
        Ok(slot.as_slice())
    }

    /// Ensures a keyed replacement can be committed within both persistent
    /// cache limits. It may resize scratch capacity, but never changes keyed
    /// history; malformed/truncated input therefore leaves the cache intact.
    fn prepare_keyed_result(&mut self, key: u64, new_len: usize) -> Result<()> {
        let old_capacity = self.store.get(&key).map_or(0, Vec::capacity);
        let new_key = !self.store.contains_key(&key);
        if new_key {
            if self.store.len() >= self.max_cache_keys {
                return Err(Error::DecodeLimitExceeded);
            }
            self.store
                .try_reserve(1)
                .map_err(|_| Error::DecodeLimitExceeded)?;
        }

        let base = self
            .cache_capacity_bytes
            .checked_sub(old_capacity)
            .ok_or(Error::InvalidData)?;
        let available = self
            .max_cache_bytes
            .checked_sub(base)
            .ok_or(Error::DecodeLimitExceeded)?;
        if new_len > available {
            return Err(Error::DecodeLimitExceeded);
        }

        // A previously large scratch would itself become persistent after the
        // swap. Replace it with a right-sized allocation when necessary so
        // shrinking values cannot consume the remaining cache budget.
        if self.result_scratch.capacity() < new_len || self.result_scratch.capacity() > available {
            let mut replacement = Vec::new();
            replacement
                .try_reserve_exact(new_len)
                .map_err(|_| Error::DecodeLimitExceeded)?;
            if replacement.capacity() > available {
                return Err(Error::DecodeLimitExceeded);
            }
            self.result_scratch = replacement;
        } else {
            self.result_scratch.clear();
        }
        Ok(())
    }

    /// Swaps the fully validated result into keyed history without allocating.
    fn commit_keyed_result(&mut self, key: u64) {
        let old_capacity = self.store.get(&key).map_or(0, Vec::capacity);
        let slot = self.store.entry(key).or_default();
        core::mem::swap(slot, &mut self.result_scratch);
        self.cache_capacity_bytes = self.cache_capacity_bytes - old_capacity + slot.capacity();
        self.peak_cache_capacity_bytes = self
            .peak_cache_capacity_bytes
            .max(self.cache_capacity_bytes);
        self.peak_cache_keys = self.peak_cache_keys.max(self.store.len());
        debug_assert!(self.cache_capacity_bytes <= self.max_cache_bytes);
        debug_assert!(self.store.len() <= self.max_cache_keys);
    }
}

/// Fallibly prepares a reusable codec scratch without exposing uninitialized
/// bytes or relying on the allocator's infallible growth path.
fn try_prepare_scratch(scratch: &mut Vec<u8>, len: usize) -> Result<()> {
    scratch.clear();
    if scratch.capacity() < len {
        let mut replacement = Vec::new();
        replacement
            .try_reserve_exact(len)
            .map_err(|_| Error::DecodeLimitExceeded)?;
        *scratch = replacement;
    }
    Ok(())
}

/// Rejects ignored XOR tail bytes that do not match XOR against the
/// zero-padded shorter value. This keeps modes 2 and 3 canonical.
fn validate_raw_xor_tail(old: &[u8], new_len: usize, xor: &[u8]) -> Result<()> {
    if new_len < old.len() && xor[new_len..] != old[new_len..] {
        return Err(Error::InvalidData);
    }
    Ok(())
}

/// Reconstructs a validated mode-3 value without exposing partial output.
fn reconstruct_raw_xor(old: &[u8], new_len: usize, xor: &[u8], result: &mut Vec<u8>) {
    result.clear();
    result.reserve(new_len);
    let overlap_len = old.len().min(new_len);
    result.extend(
        old[..overlap_len]
            .iter()
            .zip(&xor[..overlap_len])
            .map(|(&old_byte, &delta)| old_byte ^ delta),
    );
    if new_len > old.len() {
        result.extend_from_slice(&xor[overlap_len..new_len]);
    }
    debug_assert_eq!(result.len(), new_len);
}

/// Walks an RLE patch stream while checking every range before invoking
/// `visit`. Returns the number of input bytes consumed by the patches.
#[inline]
fn visit_rle_patches(
    input: &[u8],
    blob_len: usize,
    num_patches: usize,
    mut visit: impl FnMut(usize, &[u8]),
) -> Result<usize> {
    let mut cursor = Cursor::new(input);
    let mut previous_end = 0usize;

    for _ in 0..num_patches {
        let gap = decode_usize(&mut cursor)?;
        let patch_len = decode_usize(&mut cursor)?;
        let patch_start = previous_end.checked_add(gap).ok_or(Error::InvalidData)?;
        let patch_end = patch_start
            .checked_add(patch_len)
            .filter(|end| *end <= blob_len)
            .ok_or(Error::InvalidData)?;
        let remaining = cursor.buf().expect("Cursor always exposes its buffer");
        if remaining.len() < patch_len {
            return Err(Error::ReaderOutOfData);
        }
        visit(patch_start, &remaining[..patch_len]);
        cursor.advance(patch_len);
        previous_end = patch_end;
    }

    Ok(cursor.position())
}

/// Reads exactly `len` bytes into initialized storage, reusing its capacity.
/// The destination is cleared on error so partial results are not observable.
fn read_exact_into(reader: &mut impl Read, len: usize, dest: &mut Vec<u8>) -> Result<()> {
    dest.clear();
    if let Some(input) = reader.buf() {
        if input.len() < len {
            return Err(Error::ReaderOutOfData);
        }
        dest.extend_from_slice(&input[..len]);
        reader.advance(len);
        return Ok(());
    }
    dest.resize(len, 0);
    if let Err(error) = crate::io::read_exact_bytes(reader, dest) {
        dest.clear();
        return Err(error);
    }
    Ok(())
}

/// Advances by `len` bytes, borrowing a contiguous reader buffer when one is
/// available and otherwise discarding through a bounded stack buffer.
fn skip_reader_bytes(reader: &mut impl Read, len: usize) -> Result<()> {
    if let Some(input) = reader.buf() {
        if input.len() < len {
            return Err(Error::ReaderOutOfData);
        }
        reader.advance(len);
        return Ok(());
    }

    let mut remaining = len;
    let mut scratch = [0u8; 4096];
    while remaining != 0 {
        let requested = remaining.min(scratch.len());
        crate::io::read_exact_bytes(reader, &mut scratch[..requested])?;
        remaining -= requested;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::Cursor;

    #[derive(Clone, Copy)]
    enum StreamFault {
        None,
        ZeroAt(usize),
        OverreportAt(usize),
    }

    struct ChunkedReader<'a> {
        input: &'a [u8],
        position: usize,
        chunk_size: usize,
        fault: StreamFault,
    }

    impl<'a> ChunkedReader<'a> {
        fn new(input: &'a [u8], chunk_size: usize) -> Self {
            Self {
                input,
                position: 0,
                chunk_size,
                fault: StreamFault::None,
            }
        }

        fn with_fault(input: &'a [u8], chunk_size: usize, fault: StreamFault) -> Self {
            Self {
                input,
                position: 0,
                chunk_size,
                fault,
            }
        }
    }

    impl Read for ChunkedReader<'_> {
        fn read(&mut self, dest: &mut [u8]) -> Result<usize> {
            if dest.is_empty() {
                return Ok(0);
            }
            let fault_at = match self.fault {
                StreamFault::None => None,
                StreamFault::ZeroAt(offset) | StreamFault::OverreportAt(offset) => Some(offset),
            };
            if fault_at.is_some_and(|offset| self.position >= offset) {
                return match self.fault {
                    StreamFault::OverreportAt(_) => Ok(dest.len().saturating_add(1)),
                    StreamFault::ZeroAt(_) => Ok(0),
                    StreamFault::None => unreachable!(),
                };
            }
            if self.position == self.input.len() {
                return Ok(0);
            }

            let before_fault = fault_at
                .map(|offset| offset - self.position)
                .unwrap_or(usize::MAX);
            let len = dest
                .len()
                .min(self.chunk_size)
                .min(self.input.len() - self.position)
                .min(before_fault);
            dest[..len].copy_from_slice(&self.input[self.position..self.position + len]);
            self.position += len;
            Ok(len)
        }
    }

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
    fn streaming_full_frame_handles_chunks_and_preserves_state_on_bad_reads() {
        let key = 0xfeed;
        let old = vec![0x11; 64];
        let replacement = vec![0xa5; 64];

        let mut old_frame = Vec::new();
        Lencode::encode_varint_u64(0, &mut old_frame).unwrap();
        Lencode::encode_varint_u64(old.len() as u64, &mut old_frame).unwrap();
        old_frame.extend_from_slice(&old);

        let mut replacement_frame = Vec::new();
        Lencode::encode_varint_u64(0, &mut replacement_frame).unwrap();
        Lencode::encode_varint_u64(replacement.len() as u64, &mut replacement_frame).unwrap();
        let payload_offset = replacement_frame.len();
        replacement_frame.extend_from_slice(&replacement);

        let mut chunked_decoder = DiffDecoder::new();
        chunked_decoder.set_key(key);
        chunked_decoder
            .decode_blob_ref(&mut ChunkedReader::new(&old_frame, 1))
            .unwrap();
        chunked_decoder.set_key(key);
        assert_eq!(
            chunked_decoder
                .decode_blob_ref(&mut ChunkedReader::new(&replacement_frame, 1))
                .unwrap(),
            replacement
        );

        let cases = [
            (
                replacement_frame.as_slice(),
                StreamFault::ZeroAt(payload_offset + 3),
            ),
            (
                replacement_frame.as_slice(),
                StreamFault::OverreportAt(payload_offset),
            ),
            (
                &replacement_frame[..replacement_frame.len() - 1],
                StreamFault::None,
            ),
        ];
        for (input, fault) in cases {
            let mut decoder = DiffDecoder::new();
            decoder.set_key(key);
            decoder
                .decode_blob_ref(&mut Cursor::new(old_frame.as_slice()))
                .unwrap();

            decoder.set_key(key);
            let mut reader = ChunkedReader::with_fault(input, 2, fault);
            assert!(decoder.decode_blob_ref(&mut reader).is_err());
            assert_eq!(decoder.store.get(&key).unwrap(), &old);
        }
    }

    #[test]
    fn streaming_rle_patch_handles_chunks_and_preserves_state_on_bad_reads() {
        let key = 0xbeef;
        let old = vec![0x33; 128];
        let mut replacement = old.clone();
        replacement[57] = 0x77;

        let mut encoder = DiffEncoder::with_policy(DiffPolicy::RleOnly);
        encoder.set_key(key);
        let mut old_frame = Vec::new();
        encoder.encode_blob(&old, &mut old_frame).unwrap();
        encoder.set_key(key);
        let mut patch_frame = Vec::new();
        encoder.encode_blob(&replacement, &mut patch_frame).unwrap();

        let mut header = Cursor::new(patch_frame.as_slice());
        assert_eq!(Lencode::decode_varint_u64(&mut header).unwrap(), 1);
        assert_eq!(decode_usize(&mut header).unwrap(), replacement.len());
        assert_eq!(decode_usize(&mut header).unwrap(), 1);
        assert_eq!(decode_usize(&mut header).unwrap(), 57);
        assert_eq!(decode_usize(&mut header).unwrap(), 1);
        let payload_offset = header.position();

        let mut decoder = DiffDecoder::new();
        decoder.set_key(key);
        decoder
            .decode_blob_ref(&mut ChunkedReader::new(&old_frame, 1))
            .unwrap();
        decoder.set_key(key);
        assert_eq!(
            decoder
                .decode_blob_ref(&mut ChunkedReader::new(&patch_frame, 1))
                .unwrap(),
            replacement
        );

        let cases = [
            (patch_frame.as_slice(), StreamFault::ZeroAt(payload_offset)),
            (
                patch_frame.as_slice(),
                StreamFault::OverreportAt(payload_offset),
            ),
            (&patch_frame[..patch_frame.len() - 1], StreamFault::None),
        ];
        for (input, fault) in cases {
            let mut decoder = DiffDecoder::new();
            decoder.set_key(key);
            decoder
                .decode_blob_ref(&mut Cursor::new(old_frame.as_slice()))
                .unwrap();

            decoder.set_key(key);
            let mut reader = ChunkedReader::with_fault(input, 1, fault);
            assert!(decoder.decode_blob_ref(&mut reader).is_err());
            assert_eq!(decoder.store.get(&key).unwrap(), &old);
        }
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

        // Second encode uses zero patches because the data is identical.
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
    fn encoder_cache_limits_are_preflighted_and_writer_errors_preserve_history() {
        struct RejectWriter;
        impl Write for RejectWriter {
            fn write(&mut self, _buf: &[u8]) -> Result<usize> {
                Err(Error::WriterOutOfSpace)
            }

            fn flush(&mut self) -> Result<()> {
                Ok(())
            }
        }

        let mut encoder =
            DiffEncoder::with_capacity_policy_and_cache_limits(0, DiffPolicy::Disabled, 32, 8);
        encoder.set_key(1);
        assert!(matches!(
            encoder.encode_blob(&[1; 16], &mut RejectWriter),
            Err(Error::WriterOutOfSpace)
        ));
        assert!(!encoder.contains_key(1));
        assert_eq!(encoder.cache_capacity_bytes(), 0);

        let mut output = Vec::new();
        encoder.encode_blob(&[1; 16], &mut output).unwrap();
        encoder.set_key(2);
        output.clear();
        encoder.encode_blob(&[2; 16], &mut output).unwrap();
        assert_eq!(encoder.cache_capacity_bytes(), 32);

        encoder.set_key(3);
        output.clear();
        assert!(matches!(
            encoder.encode_blob(&[3], &mut output),
            Err(Error::WriterOutOfSpace)
        ));
        assert!(output.is_empty(), "cache rejection must precede output");
        assert!(!encoder.contains_key(3));
        assert_eq!(encoder.cache_capacity_bytes(), 32);
    }

    #[test]
    fn decoder_capacity_and_key_limits_block_shrink_then_rotate_amplification() {
        fn full_frame(data: &[u8]) -> Vec<u8> {
            let mut frame = Vec::new();
            Lencode::encode_varint_u64(0, &mut frame).unwrap();
            Lencode::encode_varint_u64(data.len() as u64, &mut frame).unwrap();
            frame.extend_from_slice(data);
            frame
        }

        let mut decoder = DiffDecoder::with_capacity_mode_and_cache_limits(0, 3, 32, 8);
        for (key, data) in [(1, vec![1; 16]), (1, vec![]), (2, vec![2])] {
            decoder.set_key(key);
            decoder
                .decode_blob_ref(&mut Cursor::new(full_frame(&data)))
                .unwrap();
        }
        // Key 2 inherited the recycled 16-byte scratch despite containing one
        // logical byte. Growing key 1 fills the remaining resident budget.
        decoder.set_key(1);
        decoder
            .decode_blob_ref(&mut Cursor::new(full_frame(&[3; 16])))
            .unwrap();
        assert_eq!(decoder.cache_capacity_bytes(), 32);

        decoder.set_key(3);
        let frame = full_frame(&[4]);
        assert!(matches!(
            decoder.decode_blob_ref(&mut Cursor::new(&frame)),
            Err(Error::DecodeLimitExceeded)
        ));
        assert!(!decoder.contains_key(3));
        assert_eq!(decoder.cache_capacity_bytes(), 32);

        let mut keys = DiffDecoder::with_capacity_mode_and_cache_limits(0, 3, 32, 2);
        let empty = full_frame(&[]);
        for key in [10, 11] {
            keys.set_key(key);
            keys.decode_blob_ref(&mut Cursor::new(&empty)).unwrap();
        }
        keys.set_key(12);
        assert!(matches!(
            keys.decode_blob_ref(&mut Cursor::new(&empty)),
            Err(Error::DecodeLimitExceeded)
        ));
        assert_eq!(keys.num_keys(), 2);
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

        // A diff context without a key uses normal encoding.
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

    /// The borrowed path must produce byte-identical reconstructions to the
    /// owned path across all three wire modes, including chained diffs
    /// (store state must evolve identically).
    #[test]
    fn test_decode_blob_ref_matches_decode_blob_across_modes() {
        let key = 77u64;

        // v1: full blob (mode 0); v2: single-byte change (mode 1 RLE);
        // v3: every 3rd byte changed (mode 2 XOR+zstd); v4: another RLE on
        // top of the mode-2 result, proving the store stayed consistent.
        let v1: Vec<u8> = (0..4096).map(|i| (i % 251) as u8).collect();
        let mut v2 = v1.clone();
        v2[100] ^= 0xFF;
        let mut v3 = v2.clone();
        for i in (0..v3.len()).step_by(3) {
            v3[i] = v3[i].wrapping_add(1);
        }
        let mut v4 = v3.clone();
        v4[7] ^= 0x55;

        let mut encoder = DiffEncoder::new();
        let mut buf = Vec::new();
        let mut boundaries = Vec::new();
        for data in [&v1, &v2, &v3, &v4] {
            encoder.set_key(key);
            encoder.encode_blob(data, &mut buf).unwrap();
            boundaries.push(buf.len());
        }
        // The chain must actually exercise all three modes.
        assert_eq!(buf[0], 0, "first write is a full blob");
        assert_eq!(buf[boundaries[0]], 1, "single-byte change picks RLE");
        assert_eq!(buf[boundaries[1]], 2, "scattered change picks XOR+zstd");

        let mut owned = DiffDecoder::new();
        let mut borrowed = DiffDecoder::new();
        let mut owned_cur = Cursor::new(&buf[..]);
        let mut borrowed_cur = Cursor::new(&buf[..]);
        for (expected, boundary) in [&v1, &v2, &v3, &v4].iter().zip(&boundaries) {
            owned.set_key(key);
            borrowed.set_key(key);
            let got_owned = owned.decode_blob(&mut owned_cur).unwrap();
            let got_borrowed = borrowed.decode_blob_ref(&mut borrowed_cur).unwrap();
            assert_eq!(&got_owned, *expected);
            assert_eq!(got_borrowed, expected.as_slice());
            assert_eq!(borrowed_cur.position(), *boundary);
        }
    }

    /// Without an active key, a full blob decodes into the internal scratch
    /// and diff modes fail exactly like the owned path.
    #[test]
    fn test_decode_blob_ref_without_key() {
        let data = vec![9u8; 100];
        let mut encoder = DiffEncoder::new();
        let mut buf = Vec::new();
        encoder.encode_blob(&data, &mut buf).unwrap(); // no key: full blob

        let mut decoder = DiffDecoder::new();
        let mut cursor = Cursor::new(&buf[..]);
        assert_eq!(decoder.decode_blob_ref(&mut cursor).unwrap(), &data[..]);
        assert_eq!(decoder.num_keys(), 0, "keyless blob is not stored");
    }

    /// Mode 1/2 payloads without a stored old blob must error (not panic,
    /// not fabricate) on the borrowed path, matching the owned path.
    #[test]
    fn test_decode_blob_ref_missing_old_blob_errors() {
        let old: Vec<u8> = (0..2048).map(|i| (i % 250) as u8).collect();
        let mut new = old.clone();
        new[5] ^= 0xAA;

        // Encode a chain so the second segment is a real diff...
        let mut encoder = DiffEncoder::new();
        let mut buf = Vec::new();
        encoder.set_key(1);
        encoder.encode_blob(&old, &mut buf).unwrap();
        let diff_start = buf.len();
        encoder.set_key(1);
        encoder.encode_blob(&new, &mut buf).unwrap();
        assert_eq!(buf[diff_start], 1, "second segment must be a diff");

        // ...then decode only the diff with an empty store.
        let mut decoder = DiffDecoder::new();
        decoder.set_key(1);
        let mut cursor = Cursor::new(&buf[diff_start..]);
        assert!(decoder.decode_blob_ref(&mut cursor).is_err());
    }

    #[test]
    fn test_decode_blob_ref_applies_same_length_rle_in_place() {
        let key = 91u64;
        let original: Vec<u8> = (0..4096).map(|i| (i % 251) as u8).collect();
        let mut modified = original.clone();
        modified[17] ^= 0x55;
        modified[3000] ^= 0xAA;

        let mut encoder = DiffEncoder::new();
        let mut decoder = DiffDecoder::new();

        let mut full = Vec::new();
        encoder.set_key(key);
        encoder.encode_blob(&original, &mut full).unwrap();
        decoder.set_key(key);
        let original_ptr = decoder
            .decode_blob_ref(&mut Cursor::new(&full[..]))
            .unwrap()
            .as_ptr();

        let mut diff = Vec::new();
        encoder.set_key(key);
        encoder.encode_blob(&modified, &mut diff).unwrap();
        assert_eq!(diff[0], 1, "sparse same-length update must use RLE");

        decoder.set_key(key);
        let decoded = decoder
            .decode_blob_ref(&mut Cursor::new(&diff[..]))
            .unwrap();
        assert_eq!(decoded, modified);
        assert_eq!(
            decoded.as_ptr(),
            original_ptr,
            "cached allocation is reused"
        );
    }

    #[test]
    fn test_in_place_rle_validation_preserves_cached_blob_on_error() {
        let key = 92u64;
        let original = vec![7u8; 32];
        let mut encoder = DiffEncoder::new();
        let mut decoder = DiffDecoder::new();

        let mut full = Vec::new();
        encoder.set_key(key);
        encoder.encode_blob(&original, &mut full).unwrap();
        decoder.set_key(key);
        decoder
            .decode_blob_ref(&mut Cursor::new(&full[..]))
            .unwrap();

        // mode=RLE, same length, one patch at byte 31 whose declared length
        // extends past the cached blob. Validation must fail before mutation.
        let malformed = [1u8, 32, 1, 31, 2, 0xAA, 0xBB];
        decoder.set_key(key);
        assert!(
            decoder
                .decode_blob_ref(&mut Cursor::new(&malformed[..]))
                .is_err()
        );
        assert_eq!(decoder.store.get(&key).unwrap(), &original);
    }

    #[test]
    fn test_decode_blob_ref_applies_same_length_xor_in_place() {
        let key = 93u64;
        let original: Vec<u8> = (0..4096).map(|i| (i % 251) as u8).collect();
        let mut modified = original.clone();
        for byte in modified.iter_mut().step_by(3) {
            *byte = byte.wrapping_add(1);
        }

        let mut encoder = DiffEncoder::new();
        let mut decoder = DiffDecoder::new();

        let mut full = Vec::new();
        encoder.set_key(key);
        encoder.encode_blob(&original, &mut full).unwrap();
        decoder.set_key(key);
        let original_ptr = decoder
            .decode_blob_ref(&mut Cursor::new(&full[..]))
            .unwrap()
            .as_ptr();

        let mut diff = Vec::new();
        encoder.set_key(key);
        encoder.encode_blob(&modified, &mut diff).unwrap();
        assert_eq!(diff[0], 2, "scattered same-length update must use XOR");

        decoder.set_key(key);
        let decoded = decoder
            .decode_blob_ref(&mut Cursor::new(&diff[..]))
            .unwrap();
        assert_eq!(decoded, modified);
        assert_eq!(
            decoded.as_ptr(),
            original_ptr,
            "cached allocation is reused"
        );

        // A malformed compressed frame must fail before the cached bytes are
        // touched, even though it is eligible for the in-place path.
        let malformed = [2u8, 0x82, 0x00, 0x10, 1, 0];
        decoder.set_key(key);
        assert!(
            decoder
                .decode_blob_ref(&mut Cursor::new(&malformed[..]))
                .is_err()
        );
        assert_eq!(decoder.store.get(&key).unwrap(), &modified);
    }

    #[test]
    fn diff_policy_api_preserves_defaults_history_and_store_allocations() {
        assert_eq!(DiffPolicy::default(), DiffPolicy::Adaptive);
        assert_eq!(DiffEncoder::new().policy(), DiffPolicy::Adaptive);
        assert_eq!(DiffEncoder::with_capacity(4).policy(), DiffPolicy::Adaptive);

        let key = 101u64;
        let mut encoder = DiffEncoder::with_capacity_and_policy(4, DiffPolicy::Disabled);
        assert_eq!(encoder.policy(), DiffPolicy::Disabled);
        assert!(encoder.store.capacity() >= 4);

        let first = vec![0x11; 1024];
        encoder.set_key(key);
        let mut frame = Vec::new();
        encoder.encode_blob(&first, &mut frame).unwrap();
        assert_eq!(frame[0], 0);
        let stored_ptr = encoder.store.get(&key).unwrap().as_ptr();
        let stored_capacity = encoder.store.get(&key).unwrap().capacity();

        let second = vec![0x22; 512];
        frame.clear();
        encoder.encode_blob(&second, &mut frame).unwrap();
        assert_eq!(frame[0], 0, "Disabled must emit a full frame");
        let stored = encoder.store.get(&key).unwrap();
        assert_eq!(stored, &second);
        assert_eq!(stored.as_ptr(), stored_ptr);
        assert_eq!(stored.capacity(), stored_capacity);

        // Policy changes retain the latest history, so the next diff is based
        // on the full frame the decoder most recently received.
        encoder.set_policy(DiffPolicy::OuterCompressed);
        assert_eq!(encoder.policy(), DiffPolicy::OuterCompressed);
        let mut third = second.clone();
        third[7] ^= 0xff;
        frame.clear();
        encoder.encode_blob(&third, &mut frame).unwrap();
        assert_eq!(frame[0], 1);
    }

    #[test]
    fn rle_only_policy_never_invokes_xor_modes() {
        let key = 102u64;
        let mut encoder = DiffEncoder::with_policy(DiffPolicy::RleOnly);
        encoder.set_key(key);

        let first = vec![0u8; 256];
        let mut frame = Vec::new();
        encoder.encode_blob(&first, &mut frame).unwrap();
        assert_eq!(frame[0], 0);

        let mut sparse = first.clone();
        sparse[17] = 1;
        frame.clear();
        encoder.encode_blob(&sparse, &mut frame).unwrap();
        assert_eq!(frame[0], 1);

        let dense = vec![0xff; 256];
        frame.clear();
        encoder.encode_blob(&dense, &mut frame).unwrap();
        assert_eq!(frame[0], 0, "dense changes must fall back to full");
    }

    #[test]
    fn outer_compressed_raw_xor_roundtrips_same_grow_and_shrink() {
        let key = 103u64;
        let first: Vec<u8> = (0..257).map(|index| (index % 251) as u8).collect();
        let mut same = first.clone();
        for byte in &mut same {
            *byte ^= 0xff;
        }
        let mut grown: Vec<u8> = same.iter().map(|byte| byte ^ 0x5a).collect();
        grown.extend((0..73).map(|index| 255 - index as u8));
        let shrunk: Vec<u8> = grown[..113].iter().map(|byte| byte ^ 0xa5).collect();
        let versions = [first, same, grown, shrunk];

        let mut encoder = DiffEncoder::with_policy(DiffPolicy::OuterCompressed);
        let mut frames = Vec::new();
        for (index, value) in versions.iter().enumerate() {
            encoder.set_key(key);
            let mut frame = Vec::new();
            encoder.encode_blob(value, &mut frame).unwrap();
            if index == 0 {
                assert_eq!(frame[0], 0);
            } else {
                let previous = &versions[index - 1];
                let mut cursor = Cursor::new(frame.as_slice());
                assert_eq!(Lencode::decode_varint_u64(&mut cursor).unwrap(), 3);
                assert_eq!(decode_usize(&mut cursor).unwrap(), value.len());
                assert_eq!(
                    decode_usize(&mut cursor).unwrap(),
                    previous.len().max(value.len())
                );
                assert_eq!(
                    cursor.buf().unwrap(),
                    compute_xor(previous, value),
                    "mode 3 payload must be the exact zero-padded XOR"
                );
            }
            frames.push(frame);
        }

        let mut contiguous = DiffDecoder::with_max_supported_mode(RAW_XOR_DIFF_MODE);
        #[cfg(feature = "std")]
        let mut streaming = DiffDecoder::with_max_supported_mode(RAW_XOR_DIFF_MODE);
        let mut first_contiguous_ptr = None;
        for (index, (frame, expected)) in frames.iter().zip(&versions).enumerate() {
            contiguous.set_key(key);
            let decoded = contiguous
                .decode_blob_ref(&mut Cursor::new(frame.as_slice()))
                .unwrap();
            assert_eq!(decoded, expected);
            if index == 0 {
                first_contiguous_ptr = Some(decoded.as_ptr());
            } else if index == 1 {
                assert_eq!(
                    Some(decoded.as_ptr()),
                    first_contiguous_ptr,
                    "same-length raw XOR should update cached storage in place"
                );
            }

            #[cfg(feature = "std")]
            {
                streaming.set_key(key);
                let decoded = streaming
                    .decode_blob_ref(&mut std::io::Cursor::new(frame.as_slice()))
                    .unwrap();
                assert_eq!(decoded, expected);
            }
        }
    }

    #[test]
    fn outer_compressed_bounds_sparse_large_updates_and_uses_raw_for_dense() {
        const LARGE_LEN: usize = 4 * 1024 * 1024;

        let key = 106u64;
        let old = vec![0x55; LARGE_LEN];
        let mut sparse = old.clone();
        sparse[17] ^= 1;
        sparse[LARGE_LEN - 19] ^= 2;

        let mut encoder = DiffEncoder::with_policy(DiffPolicy::OuterCompressed);
        encoder.set_key(key);
        let mut frame = Vec::new();
        encoder.encode_blob(&old, &mut frame).unwrap();
        assert_eq!(frame[0], 0);

        frame.clear();
        encoder.encode_blob(&sparse, &mut frame).unwrap();
        let expected_rle_len = plan_rle(&old, &sparse).unwrap().encoded_len;
        assert_eq!(frame[0], 1);
        assert_eq!(frame.len(), expected_rle_len);
        assert!(frame.len() < 128, "sparse frame was {} bytes", frame.len());

        let dense: Vec<u8> = sparse.iter().map(|byte| byte ^ 0xff).collect();
        frame.clear();
        encoder.encode_blob(&dense, &mut frame).unwrap();
        let expected_raw_len = 1 + varint_len(dense.len()) + varint_len(dense.len()) + dense.len();
        assert_eq!(frame[0], 3);
        assert_eq!(frame.len(), expected_raw_len);
    }

    fn raw_xor_frame(new_len: usize, xor_len: usize, xor: &[u8]) -> Vec<u8> {
        let mut frame = Vec::new();
        Lencode::encode_varint_u64(3, &mut frame).unwrap();
        Lencode::encode_varint_u64(new_len as u64, &mut frame).unwrap();
        Lencode::encode_varint_u64(xor_len as u64, &mut frame).unwrap();
        frame.extend_from_slice(xor);
        frame
    }

    fn xor_zstd_frame(new_len: usize, compressed: &[u8]) -> Vec<u8> {
        let mut frame = Vec::new();
        Lencode::encode_varint_u64(2, &mut frame).unwrap();
        Lencode::encode_varint_u64(new_len as u64, &mut frame).unwrap();
        Lencode::encode_varint_u64(compressed.len() as u64, &mut frame).unwrap();
        frame.extend_from_slice(compressed);
        frame
    }

    fn zstd_frame_with_checksum(input: &[u8]) -> Vec<u8> {
        let mut compressed = vec![0u8; zstd_safe::compress_bound(input.len())];
        let mut context = zstd_safe::CCtx::create();
        context
            .set_parameter(zstd_safe::CParameter::ChecksumFlag(true))
            .unwrap();
        let written = context.compress2(&mut compressed[..], input).unwrap();
        compressed.truncate(written);
        compressed
    }

    fn zstd_skippable_frame(payload: &[u8]) -> Vec<u8> {
        let mut frame = Vec::with_capacity(8 + payload.len());
        frame.extend_from_slice(&0x184d_2a50u32.to_le_bytes());
        frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        frame.extend_from_slice(payload);
        assert_eq!(
            zstd_safe::find_frame_compressed_size(&frame).unwrap(),
            frame.len()
        );
        frame
    }

    #[test]
    fn mode_two_rejects_concatenated_zstd_frames_without_mutating_state() {
        let key = 107u64;
        let old: Vec<u8> = (0..4096).map(|index| (index % 251) as u8).collect();
        let mut new = old.clone();
        for byte in new.iter_mut().step_by(3) {
            *byte ^= 0x5a;
        }

        let mut encoder = DiffEncoder::new();
        encoder.set_key(key);
        let mut full = Vec::new();
        encoder.encode_blob(&old, &mut full).unwrap();

        let xor = compute_xor(&old, &new);
        let first = zstd_frame_with_checksum(&xor);
        let tails = [
            zstd_frame_with_checksum(b"valid second zstd frame"),
            zstd_skippable_frame(b"valid skippable frame"),
        ];

        let mut decoder = DiffDecoder::new();
        decoder.set_key(key);
        decoder
            .decode_blob_ref(&mut Cursor::new(full.as_slice()))
            .unwrap();

        #[cfg(feature = "std")]
        let mut streaming_decoder = {
            let mut decoder = DiffDecoder::new();
            decoder.set_key(key);
            decoder
                .decode_blob_ref(&mut std::io::Cursor::new(full.as_slice()))
                .unwrap();
            decoder
        };

        for tail in tails {
            let mut concatenated = first.clone();
            concatenated.extend_from_slice(&tail);
            assert_eq!(
                zstd_safe::find_frame_compressed_size(&concatenated).unwrap(),
                first.len(),
                "the declared payload starts with one complete valid frame"
            );

            let malformed = xor_zstd_frame(new.len(), &concatenated);
            decoder.set_key(key);
            assert!(matches!(
                decoder.decode_blob_ref(&mut Cursor::new(malformed.as_slice())),
                Err(Error::InvalidData)
            ));
            assert_eq!(
                decoder.store.get(&key).unwrap(),
                &old,
                "rejection must leave keyed history unchanged"
            );

            #[cfg(feature = "std")]
            {
                streaming_decoder.set_key(key);
                assert!(matches!(
                    streaming_decoder
                        .decode_blob_ref(&mut std::io::Cursor::new(malformed.as_slice())),
                    Err(Error::InvalidData)
                ));
                assert_eq!(streaming_decoder.store.get(&key).unwrap(), &old);
            }
        }
    }

    #[test]
    fn mode_two_rejects_noncanonical_shrink_tail_without_mutating_state() {
        let key = 108u64;
        let old: Vec<u8> = (0..128).map(|index| (index % 251) as u8).collect();
        let new = &old[..48];

        let mut encoder = DiffEncoder::new();
        encoder.set_key(key);
        let mut full = Vec::new();
        encoder.encode_blob(&old, &mut full).unwrap();

        let mut xor = compute_xor(&old, new);
        xor[new.len()] ^= 1;
        let compressed = zstd_frame_with_checksum(&xor);
        let malformed = xor_zstd_frame(new.len(), &compressed);

        let mut decoder = DiffDecoder::new();
        decoder.set_key(key);
        decoder
            .decode_blob_ref(&mut Cursor::new(full.as_slice()))
            .unwrap();
        decoder.set_key(key);
        assert!(matches!(
            decoder.decode_blob_ref(&mut Cursor::new(malformed.as_slice())),
            Err(Error::InvalidData)
        ));
        assert_eq!(decoder.store.get(&key).unwrap(), &old);

        #[cfg(feature = "std")]
        {
            let mut streaming_decoder = DiffDecoder::new();
            streaming_decoder.set_key(key);
            streaming_decoder
                .decode_blob_ref(&mut std::io::Cursor::new(full.as_slice()))
                .unwrap();
            streaming_decoder.set_key(key);
            assert!(matches!(
                streaming_decoder.decode_blob_ref(&mut std::io::Cursor::new(malformed.as_slice())),
                Err(Error::InvalidData)
            ));
            assert_eq!(streaming_decoder.store.get(&key).unwrap(), &old);
        }
    }

    #[test]
    fn raw_xor_decoder_rejects_malformed_frames_without_mutating_state() {
        let key = 104u64;
        let old: Vec<u8> = (0..64).map(|index| index as u8).collect();
        let mut new = old.clone();
        for byte in new.iter_mut().step_by(5) {
            *byte ^= 0x5a;
        }

        let mut encoder = DiffEncoder::with_policy(DiffPolicy::OuterCompressed);
        encoder.set_key(key);
        let mut full = Vec::new();
        encoder.encode_blob(&old, &mut full).unwrap();
        let mut valid = Vec::new();
        encoder.encode_blob(&new, &mut valid).unwrap();
        assert_eq!(valid[0], 3);

        let mut decoder = DiffDecoder::with_max_supported_mode(RAW_XOR_DIFF_MODE);
        decoder.set_key(key);
        decoder
            .decode_blob_ref(&mut Cursor::new(full.as_slice()))
            .unwrap();

        let exact_xor = compute_xor(&old, &new);
        let wrong_short = raw_xor_frame(new.len(), exact_xor.len() - 1, &exact_xor[..63]);
        let mut wrong_long_payload = exact_xor.clone();
        wrong_long_payload.push(0);
        let wrong_long = raw_xor_frame(new.len(), exact_xor.len() + 1, &wrong_long_payload);
        let truncated = valid[..valid.len() - 1].to_vec();

        let shorter = &new[..32];
        let mut noncanonical_shrink_xor = compute_xor(&old, shorter);
        *noncanonical_shrink_xor.last_mut().unwrap() ^= 1;
        let noncanonical_shrink = raw_xor_frame(
            shorter.len(),
            noncanonical_shrink_xor.len(),
            &noncanonical_shrink_xor,
        );

        for malformed in [
            wrong_short,
            wrong_long,
            truncated.clone(),
            noncanonical_shrink,
        ] {
            decoder.set_key(key);
            assert!(
                decoder
                    .decode_blob_ref(&mut Cursor::new(malformed.as_slice()))
                    .is_err()
            );
            assert_eq!(decoder.store.get(&key).unwrap(), &old);
        }

        #[cfg(feature = "std")]
        {
            // Exercise the safe reusable-buffer fallback with a partial payload.
            decoder.set_key(key);
            assert!(
                decoder
                    .decode_blob_ref(&mut std::io::Cursor::new(truncated.as_slice()))
                    .is_err()
            );
            assert_eq!(decoder.store.get(&key).unwrap(), &old);
        }

        let mut missing_state = DiffDecoder::with_max_supported_mode(RAW_XOR_DIFF_MODE);
        missing_state.set_key(key);
        assert!(
            missing_state
                .decode_blob_ref(&mut Cursor::new(valid.as_slice()))
                .is_err()
        );
        assert!(!missing_state.contains_key(key));

        // A valid update still applies after all rejected frames.
        decoder.set_key(key);
        assert_eq!(
            decoder
                .decode_blob_ref(&mut Cursor::new(valid.as_slice()))
                .unwrap(),
            new
        );
    }

    #[test]
    fn decoder_max_mode_rejects_raw_xor_before_state_mutation() {
        let key = 105u64;
        let old = vec![0x41; 128];
        let mut new = old.clone();
        for byte in &mut new {
            *byte ^= 0xff;
        }

        let mut encoder = DiffEncoder::with_policy(DiffPolicy::OuterCompressed);
        encoder.set_key(key);
        let mut full = Vec::new();
        encoder.encode_blob(&old, &mut full).unwrap();
        let mut raw_xor = Vec::new();
        encoder.encode_blob(&new, &mut raw_xor).unwrap();
        assert_eq!(raw_xor[0], 3);

        assert_eq!(
            DiffDecoder::new().max_supported_mode(),
            DEFAULT_MAX_DIFF_MODE
        );
        assert_eq!(
            DiffDecoder::with_capacity(1).max_supported_mode(),
            DEFAULT_MAX_DIFF_MODE
        );
        let mut decoder = DiffDecoder::with_capacity_and_max_mode(1, DEFAULT_MAX_DIFF_MODE);
        assert_eq!(decoder.max_supported_mode(), DEFAULT_MAX_DIFF_MODE);
        decoder.set_key(key);
        decoder
            .decode_blob_ref(&mut Cursor::new(full.as_slice()))
            .unwrap();

        decoder.set_key(key);
        assert!(
            decoder
                .decode_blob_ref(&mut Cursor::new(raw_xor.as_slice()))
                .is_err()
        );
        assert_eq!(decoder.store.get(&key).unwrap(), &old);

        decoder.set_key(key);
        assert!(
            decoder
                .decode_blob(&mut Cursor::new(raw_xor.as_slice()))
                .is_err()
        );
        assert_eq!(decoder.store.get(&key).unwrap(), &old);

        decoder.set_max_supported_mode(RAW_XOR_DIFF_MODE);
        decoder.set_key(key);
        assert_eq!(
            decoder
                .decode_blob_ref(&mut Cursor::new(raw_xor.as_slice()))
                .unwrap(),
            new
        );
    }

    #[test]
    fn skip_diff_blob_frame_covers_every_existing_mode() {
        let key = 94u64;
        let full: Vec<u8> = (0..4096).map(|i| (i % 251) as u8).collect();
        let mut rle = full.clone();
        rle[17] ^= 0x55;
        let mut xor = rle.clone();
        for byte in xor.iter_mut().step_by(3) {
            *byte = byte.wrapping_add(1);
        }
        let mut raw_xor = xor.clone();
        for byte in &mut raw_xor {
            *byte ^= 0x33;
        }

        let mut encoder = DiffEncoder::new();
        let mut frames = Vec::new();
        let mut boundaries = vec![0usize];
        for value in [&full, &rle, &xor] {
            encoder.set_key(key);
            encoder.encode_blob(value, &mut frames).unwrap();
            boundaries.push(frames.len());
        }
        encoder.set_policy(DiffPolicy::OuterCompressed);
        encoder.set_key(key);
        encoder.encode_blob(&raw_xor, &mut frames).unwrap();
        boundaries.push(frames.len());
        assert_eq!(frames[boundaries[0]], 0);
        assert_eq!(frames[boundaries[1]], 1);
        assert_eq!(frames[boundaries[2]], 2);
        assert_eq!(frames[boundaries[3]], 3);

        for window in boundaries.windows(2) {
            let frame = &frames[window[0]..window[1]];

            // Contiguous readers take the zero-copy advance path.
            let mut contiguous = Cursor::new(frame);
            skip_diff_blob_frame_with_max_mode(&mut contiguous, RAW_XOR_DIFF_MODE).unwrap();
            assert_eq!(contiguous.position(), frame.len());

            // Generic streaming readers may return fewer bytes than requested.
            let mut chunked = ChunkedReader::new(frame, 1);
            skip_diff_blob_frame_with_max_mode(&mut chunked, RAW_XOR_DIFF_MODE).unwrap();
            assert_eq!(chunked.position, frame.len());

            #[cfg(feature = "std")]
            {
                // std::io readers exercise the bounded discard-buffer fallback.
                let mut streaming = std::io::Cursor::new(frame);
                skip_diff_blob_frame_with_max_mode(&mut streaming, RAW_XOR_DIFF_MODE).unwrap();
                assert_eq!(streaming.position(), frame.len() as u64);
            }
        }
    }

    #[test]
    fn skip_diff_blob_frame_rejects_malformed_frames() {
        let malformed: &[&[u8]] = &[
            &[0, 4, 1, 2],           // Full payload is truncated.
            &[1, 10, 1, 9, 2, 1, 2], // RLE patch extends past new_len.
            &[1, 10, 1, 1, 3, 1, 2], // RLE patch payload is truncated.
            &[2, 10, 5, 1, 2],       // Compressed payload is truncated.
            &[3, 10, 10, 1, 2],      // Raw XOR payload is truncated.
            &[3, 10, 9],             // Raw XOR cannot be shorter than output.
            &[4],                    // Unknown mode.
        ];
        for frame in malformed {
            let mut reader = Cursor::new(*frame);
            assert!(
                skip_diff_blob_frame(&mut reader).is_err(),
                "accepted {frame:02x?}"
            );
        }
    }

    #[test]
    fn skip_diff_blob_frame_respects_containing_format_mode_cap() {
        let frame = raw_xor_frame(4, 4, &[1, 2, 3, 4]);

        let mut legacy = Cursor::new(frame.as_slice());
        assert!(skip_diff_blob_frame_with_max_mode(&mut legacy, DEFAULT_MAX_DIFF_MODE).is_err());
        assert_eq!(legacy.position(), 1, "only the mode may be consumed");

        let mut current = Cursor::new(frame.as_slice());
        skip_diff_blob_frame_with_max_mode(&mut current, RAW_XOR_DIFF_MODE).unwrap();
        assert_eq!(current.position(), frame.len());
    }

    #[test]
    fn limited_reader_accepts_large_blob_without_relaxing_sequence_count() {
        const SEQUENCE_LIMIT: usize = 1_000_000;
        const BLOB_LIMIT: usize = 10 * 1024 * 1024;
        let data = vec![0x5au8; (1 << 20) + 1];
        let mut frame = Vec::new();
        DiffEncoder::new().encode_blob(&data, &mut frame).unwrap();
        assert_eq!(frame[0], 0, "unkeyed first value must be a full blob");

        let limits = crate::io::DecodeLimits::new(frame.len(), SEQUENCE_LIMIT, data.len());
        let cursor = Cursor::new(frame.as_slice());
        let mut reader =
            crate::io::LimitedReader::new(cursor, limits).with_max_blob_bytes(BLOB_LIMIT);
        assert_eq!(
            DiffDecoder::new().decode_blob_ref(&mut reader).unwrap(),
            data
        );
        assert_eq!(reader.claimed_allocation(), data.len());

        // A malicious RLE patch count remains governed by the tighter
        // collection-element limit, even though byte blobs may be larger.
        let mut malicious = Vec::new();
        Lencode::encode_varint_u64(1, &mut malicious).unwrap(); // RLE mode
        Lencode::encode_varint_u64(0, &mut malicious).unwrap(); // new blob length
        Lencode::encode_varint_u64((SEQUENCE_LIMIT + 1) as u64, &mut malicious).unwrap();
        let limits = crate::io::DecodeLimits::new(malicious.len(), SEQUENCE_LIMIT, BLOB_LIMIT);
        let cursor = Cursor::new(malicious.as_slice());
        let mut reader =
            crate::io::LimitedReader::new(cursor, limits).with_max_blob_bytes(BLOB_LIMIT);
        assert!(matches!(
            skip_diff_blob_frame(&mut reader),
            Err(Error::DecodeLimitExceeded)
        ));
    }

    #[test]
    fn skip_diff_blob_frame_honors_expanded_sequence_limit() {
        for mut frame in [vec![0, 100], vec![3, 100, 100]] {
            frame.extend_from_slice(&[7u8; 100]);
            let limits = crate::io::DecodeLimits::new(frame.len(), 99, frame.len());
            let cursor = Cursor::new(frame.as_slice());
            let mut reader = crate::io::LimitedReader::new(cursor, limits);
            assert!(matches!(
                skip_diff_blob_frame_with_max_mode(&mut reader, RAW_XOR_DIFF_MODE),
                Err(Error::DecodeLimitExceeded)
            ));
        }
    }
}
