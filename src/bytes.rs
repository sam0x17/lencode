//! Helpers for compressed byte-sequence encoding/decoding.
//!
//! This module provides zstd-based compression/decompression for contiguous u8 collections in
//! a no_std-compatible manner using `zstd-safe`.

#[cfg(not(feature = "std"))]
extern crate alloc;

use crate::prelude::*;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

/// zstd compression level used for byte-collections.
const ZSTD_LEVEL: i32 = 3;

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
