//! Helpers for compressed byte‑sequence encoding/decoding.
//!
//! This module provides zstd‑based compression/decompression for contiguous `u8` collections
//! in a `no_std`‑compatible manner using `zstd-safe`.
//!
//! An entropy heuristic ([`looks_incompressible`]) samples the first 32 bytes of a payload
//! and skips compression when the data appears random, avoiding wasted CPU on high‑entropy
//! inputs.

#[cfg(not(feature = "std"))]
extern crate alloc;

use crate::prelude::*;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

/// zstd compression level used for byte-collections.
const ZSTD_LEVEL: i32 = 1;

/// Minimum payload size to attempt compression. Below this threshold,
/// raw bytes are always used because compression overhead outweighs savings.
pub(crate) const MIN_COMPRESS_LEN: usize = 64;

/// Quick entropy check: returns `true` if a sample of the data appears incompressible.
///
/// Samples the first 32 bytes and counts distinct byte values using a 256‑bit
/// bitmap. If ≥28 out of 32 sampled bytes are distinct, the data is almost
/// certainly incompressible (e.g. random bytes, encrypted data, already‑compressed
/// content) and zstd compression is skipped.
#[inline(always)]
pub(crate) fn looks_incompressible(data: &[u8]) -> bool {
    let sample_len = data.len().min(32);
    if sample_len < 32 {
        return false; // small data: let zstd decide
    }
    // Bitmap: 256 bits = 4 u64s
    let mut bits = [0u64; 4];
    for &b in &data[..sample_len] {
        bits[(b >> 6) as usize] |= 1u64 << (b & 63);
    }
    let distinct =
        bits[0].count_ones() + bits[1].count_ones() + bits[2].count_ones() + bits[3].count_ones();
    distinct >= 28
}

/// Compresses `input` with zstd, returning the compressed bytes.
#[inline(always)]
pub fn zstd_compress(input: &[u8]) -> Result<Vec<u8>> {
    // Upper bound for compressed size
    let bound = zstd_safe::compress_bound(input.len());
    let mut out = vec![0u8; bound];
    let written = match zstd_safe::compress(&mut out[..], input, ZSTD_LEVEL) {
        Ok(n) => n,
        Err(_) => return Err(Error::InvalidData),
    };
    out.truncate(written);
    Ok(out)
}

/// Decompresses `compressed` into a new Vec<u8> with expected `original_len`.
#[inline(always)]
pub fn zstd_decompress(compressed: &[u8], original_len: usize) -> Result<Vec<u8>> {
    let mut out = vec![0u8; original_len];
    let written = match zstd_safe::decompress(&mut out[..], compressed) {
        Ok(n) => n,
        Err(_) => return Err(Error::InvalidData),
    };
    if written != original_len {
        return Err(Error::IncorrectLength);
    }
    Ok(out)
}

/// Returns the frame's declared content size, if present.
#[inline(always)]
pub fn zstd_content_size(compressed: &[u8]) -> Result<usize> {
    match zstd_safe::get_frame_content_size(compressed) {
        Ok(Some(n)) => Ok(n as usize),
        _ => Err(Error::InvalidData),
    }
}

#[inline(always)]
const fn varint_len_usize(mut val: usize) -> usize {
    if val <= 127 {
        return 1;
    }
    // count non-zero bytes in LE representation
    let mut n = 0usize;
    while val != 0 {
        n += 1;
        val >>= 8;
    }
    1 + n
}

/// Returns the number of bytes to encode the flagged length header.
///
/// The header encodes `(payload_len << 1) | (compressed as usize)` using Lencode varint.
#[inline(always)]
pub const fn flagged_header_len(payload_len: usize, compressed: bool) -> usize {
    let v = (payload_len << 1) | (compressed as usize);
    varint_len_usize(v)
}
