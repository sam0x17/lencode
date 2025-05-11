/// In-place "capped" LEB-128. This scheme uses LEB-128 up until the point where using it would
/// cost more bytes than simply using the native representation of the integer. As a result
/// this scheme will never result in a representation larger than the native one, and often can
/// compress down to a smaller number of bytes depending on the value. All values less than 127
/// are encoded using a single byte.
///
/// * `buf` must contain an **unsigned** integer in *little-endian* order.
/// * `buf.len()` can be any positive width (8, 16, 32, 64… bits; even 256, 512, …).  There is
///   no hard upper bound other than available memory.
/// * The function never allocates and never writes past the original slice.
///
/// Returns: a subslice of `buf` that is the encoded value. In-place, size-capped LEB-128.
///
/// * `buf` contains an **unsigned** integer in little-endian order.
/// * The routine never allocates and never grows the slice.
/// * Returns a subslice of `buf` that is the encoded value. In-place, size-capped LEB-128
/// encoder.
///
/// * `buf` holds an unsigned integer in little-endian order.
/// * Returns a subslice of `buf` containing the encoded value.
/// * Never grows beyond the original width; falls back to raw form if needed.
#[inline(always)]
pub fn encode_leb128_ceiling_inplace(buf: &mut [u8]) -> &[u8] {
    let w = buf.len();
    if w == 0 {
        return buf;
    }

    // locate highest non-zero byte
    let mut hi = w;
    while hi > 0 && buf[hi - 1] == 0 {
        hi -= 1;
    }

    if hi == 0 {
        buf[0] = 0; // canonical zero
        return &buf[..1];
    }

    // exact bit-length
    let msb_byte = buf[hi - 1];
    let msb_pos = 7usize - msb_byte.leading_zeros() as usize; // 0-based
    let bit_len = (hi - 1) * 8 + msb_pos + 1;

    // compute groups and ceiling check
    let groups = (bit_len + 6) / 7; // ceil(bit_len / 7)
    if groups >= w {
        // would not shrink → keep raw
        return &buf[..];
    }

    // build var-int bytes in a temp stack buffer
    //   (groups ≤ w , so this fits)
    let mut tmp = [0u8; 128]; // covers widths ≤ 1024 bits
    // debug_assert!(groups <= tmp.len());

    let mut bit_off = 0usize;
    let mut idx = 0usize;

    while bit_off < bit_len {
        let mut chunk = 0u8;
        for b in 0..7 {
            let gb = bit_off + b;
            if gb >= bit_len {
                break;
            }
            let byte_idx = gb / 8;
            let bit_idx = gb % 8;
            if (buf[byte_idx] >> bit_idx) & 1 != 0 {
                chunk |= 1 << b;
            }
        }
        let more = bit_off + 7 < bit_len;
        tmp[idx] = if more { chunk | 0x80 } else { chunk };
        idx += 1;
        bit_off += 7;
    }
    // debug_assert_eq!(idx, groups);

    // copy result back to the front of buf
    buf[..groups].copy_from_slice(&tmp[..groups]);
    &buf[..groups]
}

/// Decode the "capped" LEB‑128 produced by [`encode_leb128_ceiling_inplace`].
///
/// * `N` is the fixed byte‑width of the target integer (1, 2, 4, 8, 16, 32…).
/// * `input` may hold:
///     * the variable‑length stream (≤ N−1 bytes, terminator bit = 0), or
///     * the raw fixed‑width little‑endian integer (exactly **N** bytes).
/// * On success returns the value as a little‑endian `[u8; N]`.  
///   Returns `None` on malformed input (unterminated, overflow, etc.). Decode the capped
/// LEB-128 produced above.
///
/// * `input` is either the variable-length stream (≤ N−1 bytes, MSB 0 on last) or the raw
///   fixed-width value (exactly N bytes).
/// * On success returns the little-endian bytes of width **N**.
#[inline(always)]
pub fn decode_leb128_ceiling<const N: usize>(input: &[u8]) -> Option<[u8; N]> {
    if N == 0 {
        return None;
    }

    // fast path: raw ceiling form
    if input.len() == N {
        let mut out = [0u8; N];
        out.copy_from_slice(input);
        return Some(out);
    }

    if input.is_empty() || input.len() >= N {
        return None;
    }

    let mut out = [0u8; N];
    let mut bit_offset = 0usize;

    for (i, &byte) in input.iter().enumerate() {
        let payload = byte & 0x7F;

        // write 7 payload bits
        for b in 0..7 {
            if payload & (1 << b) != 0 {
                let gb = bit_offset + b;
                if gb >= N * 8 {
                    return None;
                } // overflow
                let byte_idx = gb / 8;
                let bit_idx = gb % 8;
                out[byte_idx] |= 1 << bit_idx;
            }
        }

        bit_offset += 7;

        // last byte? (MSB 0)
        if byte & 0x80 == 0 {
            if i + 1 != input.len() {
                return None;
            } // junk after end
            return Some(out);
        }
    }
    None // unterminated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[inline(always)]
    fn roundtrip<const N: usize>(bytes: [u8; N]) {
        let mut buf = bytes;
        let enc = encode_leb128_ceiling_inplace(&mut buf);
        let dec = decode_leb128_ceiling::<N>(enc).expect("decode failed");

        assert_eq!(dec, bytes, "round-trip mismatch for {bytes:?}");
        assert!(enc.len() <= N, "encoded length exceeds width");
    }

    // exhaustive u8
    #[test]
    fn u8_all() {
        for v in 0u8..=u8::MAX {
            roundtrip::<1>(v.to_le_bytes());
        }
    }

    // exhaustive u16
    #[test]
    fn u16_all() {
        for v in 0u16..=u16::MAX {
            roundtrip::<2>(v.to_le_bytes());
        }
    }

    #[test]
    fn u32_all() {
        use rayon::prelude::*;
        (0u32..=u32::MAX).into_par_iter().for_each(|v| {
            roundtrip::<4>(v.to_le_bytes());
        });
    }

    // u32 boundaries
    #[test]
    fn u32_boundaries() {
        let s = [
            0,
            1,
            127,
            128,
            16_383,
            16_384,
            (1 << 21) - 1,
            1 << 21,
            (1 << 28) - 1,
            1 << 28,
            u32::MAX,
        ];
        for &v in &s {
            roundtrip::<4>(v.to_le_bytes());
        }
    }

    // u64 edges
    #[test]
    fn u64_edges() {
        let s = [
            0,
            1,
            127,
            128,
            (1 << 14) - 1,
            1 << 14,
            (1 << 21) - 1,
            1 << 21,
            (1 << 28) - 1,
            1 << 28,
            (1 << 49) - 1,
            1 << 49,
            1 << 56,
            (1 << 63) - 1,
            1 << 63,
            u64::MAX,
        ];
        for &v in &s {
            roundtrip::<8>(v.to_le_bytes());
        }
    }

    #[test]
    fn u128_selected() {
        let s = [0u128, 1, 127, ((1u128 << 127) - 1), 1u128 << 127];
        for &v in &s {
            roundtrip::<16>(v.to_le_bytes());
        }
    }

    // 256-bit buffer tests
    #[test]
    fn u256_buffer() {
        // zero compresses
        roundtrip::<32>([0u8; 32]);

        // mid-range pattern
        let mut mid = [0u8; 32];
        for i in 0..32 {
            mid[i] = 0xEFu8.wrapping_add(i as u8).wrapping_mul(0x11);
        }
        roundtrip::<32>(mid);

        // near-max (top bit 0) triggers ceiling
        let mut near_max = [0xFF; 32];
        near_max[31] = 0x7F;
        roundtrip::<32>(near_max);
    }
}
