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
///
/// A fast pre‑check inspects the byte range (max − min) of the first 8 bytes.
/// Random/encrypted data almost always spans > 200 of the 256 byte values, so
/// a wide range is a strong signal of incompressibility and lets us skip the
/// full 32‑byte bitmap scan. The pre‑check uses only min/max — no bitmap, no
/// popcount, no array indexing — so it adds negligible overhead (~3 ns) to the
/// fallthrough path for genuinely compressible data.
#[inline(always)]
pub(crate) fn looks_incompressible(data: &[u8]) -> bool {
    if data.len() < 32 {
        return false;
    }
    // Pre-check: byte range of the first 8 bytes. For uniformly random
    // data the expected range is ~213 out of 256, so a threshold of 200
    // catches ~80% of random payloads in ~3 ns.  Uniform data (range 0),
    // ASCII text (range ≤ 94), and other compressible patterns fall
    // through to the full scan below.
    let (mut lo, mut hi) = (data[0], data[0]);
    for &b in &data[1..8] {
        lo = lo.min(b);
        hi = hi.max(b);
    }
    if hi.wrapping_sub(lo) > 200 {
        return true;
    }
    // Full 32‑byte bitmap scan.
    let mut bits = [0u64; 4];
    for &b in &data[..32] {
        bits[(b >> 6) as usize] |= 1u64 << (b & 63);
    }
    let distinct =
        bits[0].count_ones() + bits[1].count_ones() + bits[2].count_ones() + bits[3].count_ones();
    distinct >= 28
}

// Thread-local zstd state (std only): a single TLS slot holds the CCtx, the
// DCtx, and a reusable scratch Vec, so each compression/decompression call
// pays exactly one TLS lookup rather than two. Reusing a CCtx/DCtx is
// significantly faster for small payloads because internal allocations are
// reused, and the scratch buffer eliminates the per-call Vec allocation for
// the compressed output.
//
// Wire-format note: zstd at a given compression level produces deterministic
// output for a given input regardless of whether the context is freshly created
// or reused. Therefore the std (CCtx) and no_std (fresh-context) paths produce
// identical compressed bytes for the same input, and the encoded wire format is
// identical across builds. This invariant is exercised by `wire_format_*` tests.
#[cfg(feature = "std")]
struct ZstdState {
    cctx: zstd_safe::CCtx<'static>,
    dctx: zstd_safe::DCtx<'static>,
    scratch: Vec<u8>,
}

#[cfg(feature = "std")]
thread_local! {
    static ZSTD_STATE: core::cell::RefCell<ZstdState> = core::cell::RefCell::new(ZstdState {
        cctx: zstd_safe::CCtx::create(),
        dctx: zstd_safe::DCtx::create(),
        scratch: Vec::new(),
    });
}

/// Compresses `input` and writes the flagged compressed payload directly into
/// `writer`. Returns `Ok(Some(total))` on success, `Ok(None)` if compression
/// did not beat raw size (caller should encode raw instead).
///
/// Internally uses a thread-local scratch buffer and CCtx when std is available
/// to avoid per-call allocation and context creation; falls back to a fresh
/// Vec + fresh CCtx for no_std. The wire format produced is identical in both
/// cases (see module docs).
///
/// Marked `#[cold]` and `#[inline(never)]` so the compression-branch body
/// stays out of the hot encode paths. Random / short payloads never call
/// this, so keeping it out-of-line prevents code bloat that would otherwise
/// push unrelated hot functions (e.g. decode entry points) out of the i-cache.
#[cold]
#[inline(never)]
pub(crate) fn compress_and_write(
    input: &[u8],
    writer: &mut impl crate::io::Write,
) -> Result<Option<usize>> {
    let raw_len = input.len();
    let raw_hdr_len = flagged_header_len(raw_len, false);
    let bound = zstd_safe::compress_bound(raw_len);

    #[cfg(feature = "std")]
    {
        ZSTD_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            // `Vec::reserve` is relative to `len`, not `capacity` — clear
            // first so `reserve(bound)` guarantees `capacity >= bound`. The
            // previous arithmetic (`reserve(bound - capacity)`) under-reserved
            // whenever an earlier smaller-bound call had left `len < capacity`:
            // `set_len(bound)` then exceeded the allocation and zstd wrote
            // past the end of the heap block (caught by ASAN on real mainnet
            // transaction payloads).
            state.scratch.clear();
            if state.scratch.capacity() < bound {
                state.scratch.reserve(bound);
            }
            // SAFETY: capacity is at least `bound`; zstd writes only the bytes
            // it returns, and we slice to that length below.
            unsafe {
                state.scratch.set_len(bound);
            }
            // Borrow split so we can hold mutable refs to both fields simultaneously.
            let ZstdState {
                ref mut cctx,
                ref mut scratch,
                ..
            } = *state;
            let comp_len = cctx
                .compress(&mut scratch[..], input, ZSTD_LEVEL)
                .map_err(|_| Error::InvalidData)?;
            let comp_hdr_len = flagged_header_len(comp_len, true);
            if comp_len + comp_hdr_len >= raw_len + raw_hdr_len {
                return Ok(None);
            }
            Ok(Some(write_flagged_raw(writer, &scratch[..comp_len], 1)?))
        })
    }
    #[cfg(not(feature = "std"))]
    {
        // SAFETY: same as `zstd_compress` — zstd writes into the buffer and we
        // only slice up to the reported `comp_len`.
        #[allow(clippy::uninit_vec)]
        let mut scratch: Vec<u8> = unsafe {
            let mut v = Vec::with_capacity(bound);
            v.set_len(bound);
            v
        };
        let comp_len = zstd_safe::compress(&mut scratch[..], input, ZSTD_LEVEL)
            .map_err(|_| Error::InvalidData)?;
        let comp_hdr_len = flagged_header_len(comp_len, true);
        if comp_len + comp_hdr_len >= raw_len + raw_hdr_len {
            return Ok(None);
        }
        Ok(Some(write_flagged_raw(writer, &scratch[..comp_len], 1)?))
    }
}

/// Compresses `input` with zstd, returning the compressed bytes. Used by the
/// no_std-compatible path and by callers (such as the diff encoder) that need
/// an owned output buffer.
///
/// `#[inline(never)]` is deliberate: inlining this pulls the thread-local
/// CCtx lookup and borrow machinery into every caller, which inflates hot
/// byte-encode paths whose compression branch is rarely taken and pushes
/// unrelated hot functions out of the i-cache.
#[inline(never)]
pub fn zstd_compress(input: &[u8]) -> Result<Vec<u8>> {
    let bound = zstd_safe::compress_bound(input.len());
    // SAFETY: `out` is immediately passed to `zstd_safe::compress` which writes
    // into `out[..written]`. We `truncate(written)` before returning, so callers
    // never observe the uninitialized tail. Skipping zero-init saves a memset
    // that zstd would overwrite anyway.
    #[allow(clippy::uninit_vec)]
    let mut out: Vec<u8> = unsafe {
        let mut v = Vec::with_capacity(bound);
        v.set_len(bound);
        v
    };
    #[cfg(feature = "std")]
    let written = ZSTD_STATE
        .with(|cell| {
            cell.borrow_mut()
                .cctx
                .compress(&mut out[..], input, ZSTD_LEVEL)
        })
        .map_err(|_| Error::InvalidData)?;
    #[cfg(not(feature = "std"))]
    let written =
        zstd_safe::compress(&mut out[..], input, ZSTD_LEVEL).map_err(|_| Error::InvalidData)?;
    out.truncate(written);
    Ok(out)
}

/// Decompresses `compressed` into a new Vec<u8> with expected `original_len`.
///
/// `#[inline(never)]` is deliberate — same reasoning as [`zstd_compress`].
/// The raw-bytes decode path inside `Vec<u8>::decode_ext` is the hot path,
/// and inlining the thread-local DCtx lookup + borrow machinery here
/// measurably regressed the `solana_message_decode` bench (~+6%) by bloating
/// that inner loop's code footprint.
#[inline(never)]
pub fn zstd_decompress(compressed: &[u8], original_len: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    zstd_decompress_into(compressed, original_len, &mut out)?;
    Ok(out)
}

/// [`zstd_decompress`] into a caller-provided buffer whose capacity is
/// reused — the allocation-free variant for callers that decompress in a
/// loop (e.g. the diff decoder's borrowed path). `out` is cleared first and
/// holds exactly the decompressed bytes on success (cleared on error).
///
/// `#[inline(never)]` — same i-cache reasoning as [`zstd_decompress`].
#[inline(never)]
pub fn zstd_decompress_into(
    compressed: &[u8],
    original_len: usize,
    out: &mut Vec<u8>,
) -> Result<()> {
    out.clear();
    out.reserve(original_len);
    // SAFETY: `out` is immediately passed to `zstd_safe::decompress` which
    // writes exactly `original_len` bytes (verified below). On every error
    // path we clear `out`, so callers never observe the uninitialized
    // region. Skipping zero-init saves a memset zstd would overwrite anyway.
    #[allow(clippy::uninit_vec)]
    unsafe {
        out.set_len(original_len);
    }
    #[cfg(feature = "std")]
    let written =
        ZSTD_STATE.with(|cell| cell.borrow_mut().dctx.decompress(&mut out[..], compressed));
    #[cfg(not(feature = "std"))]
    let written = zstd_safe::decompress(&mut out[..], compressed);
    let written = match written {
        Ok(written) => written,
        Err(_) => {
            out.clear();
            return Err(Error::InvalidData);
        }
    };
    if written != original_len {
        out.clear();
        return Err(Error::IncorrectLength);
    }
    Ok(())
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

/// Writes a raw flagged byte payload (header + data) into `writer` in one shot,
/// pre-reserving capacity to avoid double allocation. The header encodes
/// `(payload_len << 1) | flag` using a Lencode varint.
#[inline(always)]
pub(crate) fn write_flagged_raw(
    writer: &mut impl crate::io::Write,
    bytes: &[u8],
    flag: usize,
) -> Result<usize> {
    let raw_len = bytes.len();
    let header_val = ((raw_len << 1) | flag) as u64;
    // Compute header length using leading_zeros (matches encode_varint_u64)
    let hdr_len = if header_val <= 0x7F {
        1usize
    } else {
        1 + (((64 - header_val.leading_zeros() + 7) >> 3) as usize)
    };
    let total = hdr_len + raw_len;
    writer.reserve(total);
    if let Some(dst) = writer.buf_mut()
        && dst.len() >= total
    {
        unsafe {
            let p = dst.as_mut_ptr();
            if header_val <= 0x7F {
                *p = header_val as u8;
                core::ptr::copy_nonoverlapping(bytes.as_ptr(), p.add(1), raw_len);
            } else {
                let n = hdr_len - 1;
                *p = 0x80 | (n as u8);
                (p.add(1) as *mut u64).write_unaligned(header_val.to_le());
                core::ptr::copy_nonoverlapping(bytes.as_ptr(), p.add(hdr_len), raw_len);
            }
        }
        writer.advance_mut(total);
        return Ok(total);
    }
    // Fallback: write through trait
    let mut out = [0u8; 9];
    if header_val <= 0x7F {
        out[0] = header_val as u8;
    } else {
        let n = hdr_len - 1;
        out[0] = 0x80 | (n as u8);
        let le = header_val.to_le_bytes();
        unsafe {
            core::ptr::copy_nonoverlapping(le.as_ptr(), out.as_mut_ptr().add(1), n);
        }
    }
    writer.write(&out[..hdr_len])?;
    writer.write(bytes)?;
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::VecWriter;
    use crate::prelude::Encode;

    /// Verifies that compressing the same input via the thread-local CCtx
    /// (used in the std fast path) produces the same bytes as a fresh
    /// per-call compression. Both feed into the same wire format helper, so
    /// std and no_std builds produce identical encoded bytes.
    #[test]
    fn wire_format_compress_is_deterministic() {
        let payload = vec![0u8; 512];
        // Path A: thread-local CCtx via compress_and_write
        let mut writer_a = VecWriter::new();
        let _ = compress_and_write(&payload, &mut writer_a)
            .expect("compress_and_write")
            .expect("compression should beat raw for all-zero payload");
        // Path B: fresh-context compression using the bare zstd_safe API,
        // mirroring the no_std fallback.
        let bound = zstd_safe::compress_bound(payload.len());
        let mut tmp = vec![0u8; bound];
        let written =
            zstd_safe::compress(&mut tmp[..], &payload, ZSTD_LEVEL).expect("zstd_safe::compress");
        tmp.truncate(written);
        let mut writer_b = VecWriter::new();
        write_flagged_raw(&mut writer_b, &tmp, 1).unwrap();
        assert_eq!(
            writer_a.into_inner(),
            writer_b.into_inner(),
            "thread-local CCtx and fresh-context paths must produce identical wire bytes"
        );
    }

    /// Round-trip test: encode through the fast path, decode through the
    /// existing path. Catches any divergence in the wire format that would
    /// break interop.
    #[test]
    fn wire_format_compressible_round_trip() {
        let payload = vec![7u8; 1024];
        let mut writer = VecWriter::new();
        Encode::encode_ext(&payload, &mut writer, None).unwrap();
        let bytes = writer.into_inner();
        let decoded: Vec<u8> = crate::decode(&mut crate::io::Cursor::new(&bytes)).unwrap();
        assert_eq!(decoded, payload);
    }

    /// Encoding the same payload twice in a row must produce identical output
    /// — the thread-local CCtx must not retain state across calls that affects
    /// compressed output.
    #[test]
    fn wire_format_repeated_encode_stable() {
        let payload = b"the quick brown fox jumps over the lazy dog the quick brown fox jumps over the lazy dog the quick brown fox".to_vec();
        let mut w1 = VecWriter::new();
        Encode::encode_ext(&payload, &mut w1, None).unwrap();
        let mut w2 = VecWriter::new();
        Encode::encode_ext(&payload, &mut w2, None).unwrap();
        assert_eq!(w1.into_inner(), w2.into_inner());
    }
}

#[cfg(all(test, feature = "std"))]
mod scratch_reserve_regression {
    use super::*;

    /// Reproduces the scratch under-reservation overflow: a first call sizes
    /// the scratch allocation exactly to its bound, a second smaller call
    /// leaves `len < capacity`, and a third call whose bound exceeds capacity
    /// used to under-reserve (reserve is len-relative), hand zstd phantom
    /// capacity via `set_len`, and let it write past the heap block. With the
    /// fix, every call sees a correctly sized scratch; under ASAN the old
    /// code fails this test with a heap-buffer-overflow.
    #[test]
    fn scratch_regrows_after_smaller_bound_call() {
        let mut sink = crate::io::VecWriter::default();

        // Call 1: compressible, establishes scratch capacity ~= bound(777).
        let a = vec![0xAB_u8; 777];
        compress_and_write(&a, &mut sink).unwrap();

        // Call 2: smaller compressible payload drops scratch len below
        // capacity.
        let b = vec![0xCD_u8; 200];
        compress_and_write(&b, &mut sink).unwrap();

        // Call 3: incompressible payload whose bound exceeds capacity but
        // whose (old, wrong) reserve request fit inside it. zstd's attempt
        // expands the input and must have real room to do so.
        let mut x: u32 = 0x1234_5678;
        let c: Vec<u8> = (0..810)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 24) as u8
            })
            .collect();
        let res = compress_and_write(&c, &mut sink).unwrap();
        assert!(res.is_none(), "incompressible payload must not be kept");

        // Scratch must remain coherent for further use.
        let d = vec![0xEF_u8; 4096];
        compress_and_write(&d, &mut sink).unwrap();
    }
}
