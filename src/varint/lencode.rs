use crate::prelude::*;
#[cfg(test)]
use core::mem;

/// Reconstruct an [`UnsignedInteger`] from a slice of little-endian bytes.
///
/// Works on all endiannesses by building the value through shifts and ORs.
#[cfg(target_endian = "big")]
#[inline(always)]
fn from_le_bytes<I: UnsignedInteger>(le: &[u8]) -> I {
    let mut val = I::ZERO;
    let mut base = I::ONE;
    for (i, &byte) in le.iter().enumerate() {
        if byte != 0 {
            let mut part = I::ZERO;
            let mut factor = base;
            let mut c = byte;
            while c != 0 {
                if (c & 1) != 0 {
                    part += factor;
                }
                factor = factor << 1;
                c >>= 1;
            }
            val += part;
        }
        if i + 1 < le.len() {
            base = base << 8;
        }
    }
    val
}

/// Validates a length-prefixed native lencode header for a target width.
#[inline(always)]
fn decode_payload_len(first: u8, max_bytes: usize) -> Result<usize> {
    let len = usize::from(first & 0x7f);
    if first & 0x80 == 0 || len == 0 || len > max_bytes {
        return Err(Error::InvalidData);
    }
    Ok(len)
}

/// Rejects redundant high zero bytes and values that belong in the one-byte
/// small-integer representation.
#[inline(always)]
fn validate_canonical_payload(payload: &[u8]) -> Result<()> {
    let &last = payload.last().ok_or(Error::InvalidData)?;
    if last == 0 || (payload.len() == 1 && last <= 0x7f) {
        return Err(Error::InvalidData);
    }
    Ok(())
}

/// The Lencode integer encoding scheme is designed to encode integers in a variable‑length
/// format that is efficient for both small and large values both in terms of space and speed.
///
/// Lencode is a hybrid scheme where small integers <= 127 are encoded in a single byte (the
/// first bit is a flag indicating whether the integer is small or large, 0 means small and 1
/// means large). Large integers > 127 have the length of their raw bytes encoded in the
/// remaining 7 bits of the first byte, followed by the raw bytes of the integer. In this way
/// we never waste more than one byte for large integers, and small integers always fit within
/// a single byte. The only case where we waste more than the full byte size of an integer
/// primitive is when the value is large enough to require 1s in the most significant byte, in
/// which case we waste one additional byte for the length encoding.
///
/// Integers that need more than 127 bytes in their standard two's complement representation
/// are not supported by this scheme, but such integers are incredibly large and unlikely to be
/// used in practice.
pub enum Lencode {}

impl Lencode {
    #[inline(always)]
    pub(crate) fn encode_varint_u16(val: u16, writer: &mut impl Write) -> Result<usize> {
        // One upfront length check covers every zero-copy case.
        if let Some(dst) = writer.buf_mut() {
            if dst.len() >= 3 {
                if val <= 0x7F {
                    unsafe { *dst.get_unchecked_mut(0) = val as u8 };
                    writer.advance_mut(1);
                    return Ok(1);
                }
                let n = ((16 - val.leading_zeros() + 7) >> 3) as usize;
                unsafe {
                    *dst.get_unchecked_mut(0) = 0x80 | (n as u8);
                    (dst.as_mut_ptr().add(1) as *mut [u8; 2]).write_unaligned(val.to_le_bytes());
                }
                writer.advance_mut(1 + n);
                return Ok(1 + n);
            }
            // Short buffer path
            if val <= 0x7F {
                if dst.is_empty() {
                    return Err(Error::WriterOutOfSpace);
                }
                unsafe { *dst.get_unchecked_mut(0) = val as u8 };
                writer.advance_mut(1);
                return Ok(1);
            }
            let n = ((16 - val.leading_zeros() + 7) >> 3) as usize;
            let total = 1 + n;
            if dst.len() < total {
                return Err(Error::WriterOutOfSpace);
            }
            let le = val.to_le_bytes();
            unsafe {
                *dst.get_unchecked_mut(0) = 0x80 | (n as u8);
                core::ptr::copy_nonoverlapping(le.as_ptr(), dst.as_mut_ptr().add(1), n);
            }
            writer.advance_mut(total);
            return Ok(total);
        }
        // Fallback
        if val <= 0x7F {
            let byte = val as u8;
            writer.write(core::slice::from_ref(&byte))?;
            return Ok(1);
        }
        let n = ((16 - val.leading_zeros() + 7) >> 3) as usize;
        let mut out = [0u8; 3];
        out[0] = 0x80 | (n as u8);
        let le = val.to_le_bytes();
        unsafe {
            (out.as_mut_ptr().add(1) as *mut [u8; 2]).write_unaligned(le);
        }
        writer.write(&out[..(1 + n)])?;
        Ok(1 + n)
    }

    #[inline(always)]
    pub(crate) fn encode_varint_u32(val: u32, writer: &mut impl Write) -> Result<usize> {
        // One upfront length check covers every zero-copy case.
        if let Some(dst) = writer.buf_mut() {
            if dst.len() >= 5 {
                if val <= 0x7F {
                    unsafe { *dst.get_unchecked_mut(0) = val as u8 };
                    writer.advance_mut(1);
                    return Ok(1);
                }
                let n = ((32 - val.leading_zeros() + 7) >> 3) as usize;
                unsafe {
                    *dst.get_unchecked_mut(0) = 0x80 | (n as u8);
                    (dst.as_mut_ptr().add(1) as *mut [u8; 4]).write_unaligned(val.to_le_bytes());
                }
                writer.advance_mut(1 + n);
                return Ok(1 + n);
            }
            // Short buffer path
            if val <= 0x7F {
                if dst.is_empty() {
                    return Err(Error::WriterOutOfSpace);
                }
                unsafe { *dst.get_unchecked_mut(0) = val as u8 };
                writer.advance_mut(1);
                return Ok(1);
            }
            let n = ((32 - val.leading_zeros() + 7) >> 3) as usize;
            let total = 1 + n;
            if dst.len() < total {
                return Err(Error::WriterOutOfSpace);
            }
            let le = val.to_le_bytes();
            unsafe {
                *dst.get_unchecked_mut(0) = 0x80 | (n as u8);
                core::ptr::copy_nonoverlapping(le.as_ptr(), dst.as_mut_ptr().add(1), n);
            }
            writer.advance_mut(total);
            return Ok(total);
        }
        // Fallback
        if val <= 0x7F {
            let byte = val as u8;
            writer.write(core::slice::from_ref(&byte))?;
            return Ok(1);
        }
        let n = ((32 - val.leading_zeros() + 7) >> 3) as usize;
        let mut out = [0u8; 5];
        out[0] = 0x80 | (n as u8);
        let le = val.to_le_bytes();
        unsafe {
            (out.as_mut_ptr().add(1) as *mut [u8; 4]).write_unaligned(le);
        }
        writer.write(&out[..(1 + n)])?;
        Ok(1 + n)
    }

    #[inline(always)]
    pub(crate) fn encode_varint_u64(val: u64, writer: &mut impl Write) -> Result<usize> {
        // One upfront length check covers every zero-copy case.
        if let Some(dst) = writer.buf_mut() {
            if dst.len() >= 9 {
                if val <= 0x7F {
                    unsafe { *dst.get_unchecked_mut(0) = val as u8 };
                    writer.advance_mut(1);
                    return Ok(1);
                }
                let n = ((64 - val.leading_zeros() + 7) >> 3) as usize;
                unsafe {
                    *dst.get_unchecked_mut(0) = 0x80 | (n as u8);
                    (dst.as_mut_ptr().add(1) as *mut [u8; 8]).write_unaligned(val.to_le_bytes());
                }
                writer.advance_mut(1 + n);
                return Ok(1 + n);
            }
            // Short buffer path
            if val <= 0x7F {
                if dst.is_empty() {
                    return Err(Error::WriterOutOfSpace);
                }
                unsafe { *dst.get_unchecked_mut(0) = val as u8 };
                writer.advance_mut(1);
                return Ok(1);
            }
            let n = ((64 - val.leading_zeros() + 7) >> 3) as usize;
            let total = 1 + n;
            if dst.len() < total {
                return Err(Error::WriterOutOfSpace);
            }
            let le = val.to_le_bytes();
            unsafe {
                *dst.get_unchecked_mut(0) = 0x80 | (n as u8);
                core::ptr::copy_nonoverlapping(le.as_ptr(), dst.as_mut_ptr().add(1), n);
            }
            writer.advance_mut(total);
            return Ok(total);
        }
        // Fallback
        if val <= 0x7F {
            let byte = val as u8;
            writer.write(core::slice::from_ref(&byte))?;
            return Ok(1);
        }
        let n = ((64 - val.leading_zeros() + 7) >> 3) as usize;
        let mut out = [0u8; 9];
        out[0] = 0x80 | (n as u8);
        let le = val.to_le_bytes();
        unsafe {
            (out.as_mut_ptr().add(1) as *mut [u8; 8]).write_unaligned(le);
        }
        writer.write(&out[..(1 + n)])?;
        Ok(1 + n)
    }

    #[inline(always)]
    pub(crate) fn encode_varint_u128(val: u128, writer: &mut impl Write) -> Result<usize> {
        // One upfront length check covers every zero-copy case.
        if let Some(dst) = writer.buf_mut() {
            if dst.len() >= 17 {
                if val <= 0x7F {
                    unsafe { *dst.get_unchecked_mut(0) = val as u8 };
                    writer.advance_mut(1);
                    return Ok(1);
                }
                // Split into hi/lo halves to avoid u128.leading_zeros() which
                // compiles to a branch + two bsr instructions on x86-64.
                let lo = val as u64;
                let hi = (val >> 64) as u64;
                let n = if hi != 0 {
                    8 + (((64 - hi.leading_zeros() + 7) >> 3) as usize)
                } else {
                    ((64 - lo.leading_zeros() + 7) >> 3) as usize
                };
                unsafe {
                    *dst.get_unchecked_mut(0) = 0x80 | (n as u8);
                    let ptr = dst.as_mut_ptr().add(1);
                    (ptr as *mut u64).write_unaligned(lo.to_le());
                    (ptr.add(8) as *mut u64).write_unaligned(hi.to_le());
                }
                writer.advance_mut(1 + n);
                return Ok(1 + n);
            }
            // Short buffer path
            if val <= 0x7F {
                if dst.is_empty() {
                    return Err(Error::WriterOutOfSpace);
                }
                unsafe { *dst.get_unchecked_mut(0) = val as u8 };
                writer.advance_mut(1);
                return Ok(1);
            }
            let n = ((128 - val.leading_zeros() + 7) >> 3) as usize;
            let total = 1 + n;
            if dst.len() < total {
                return Err(Error::WriterOutOfSpace);
            }
            let le = val.to_le_bytes();
            unsafe {
                *dst.get_unchecked_mut(0) = 0x80 | (n as u8);
                core::ptr::copy_nonoverlapping(le.as_ptr(), dst.as_mut_ptr().add(1), n);
            }
            writer.advance_mut(total);
            return Ok(total);
        }
        // Fallback
        if val <= 0x7F {
            let byte = val as u8;
            writer.write(core::slice::from_ref(&byte))?;
            return Ok(1);
        }
        let n = ((128 - val.leading_zeros() + 7) >> 3) as usize;
        let mut out = [0u8; 17];
        out[0] = 0x80 | (n as u8);
        let le = val.to_le_bytes();
        unsafe {
            (out.as_mut_ptr().add(1) as *mut [u8; 16]).write_unaligned(le);
        }
        writer.write(&out[..(1 + n)])?;
        Ok(1 + n)
    }

    #[inline(always)]
    pub(crate) fn encode_varint_i16(val: i16, writer: &mut impl Write) -> Result<usize> {
        Self::encode_varint_u16(zigzag_encode(val), writer)
    }

    #[inline(always)]
    pub(crate) fn encode_varint_i32(val: i32, writer: &mut impl Write) -> Result<usize> {
        Self::encode_varint_u32(zigzag_encode(val), writer)
    }

    #[inline(always)]
    pub(crate) fn encode_varint_i64(val: i64, writer: &mut impl Write) -> Result<usize> {
        Self::encode_varint_u64(zigzag_encode(val), writer)
    }

    #[inline(always)]
    pub(crate) fn encode_varint_i128(val: i128, writer: &mut impl Write) -> Result<usize> {
        Self::encode_varint_u128(zigzag_encode(val), writer)
    }

    #[inline(always)]
    pub(crate) fn decode_varint_u16(reader: &mut impl Read) -> Result<u16> {
        if let Some(slice) = reader.buf() {
            let first = *slice.first().ok_or(Error::ReaderOutOfData)?;
            if first & 0x80 == 0 {
                reader.advance(1);
                return Ok(u16::from(first));
            }
            let n = decode_payload_len(first, 2)?;
            let payload = slice.get(1..1 + n).ok_or(Error::ReaderOutOfData)?;
            validate_canonical_payload(payload)?;

            let value = if slice.len() >= 3 {
                let raw =
                    u16::from_le(unsafe { (slice.as_ptr().add(1) as *const u16).read_unaligned() });
                if n < 2 {
                    raw & ((1u16 << (n << 3)) - 1)
                } else {
                    raw
                }
            } else {
                let mut bytes = [0u8; 2];
                bytes[..n].copy_from_slice(payload);
                u16::from_le_bytes(bytes)
            };
            reader.advance(1 + n);
            return Ok(value);
        }

        let mut first = [0u8; 1];
        crate::io::read_exact_bytes(reader, &mut first)?;
        let first = first[0];
        if first & 0x80 == 0 {
            return Ok(u16::from(first));
        }
        let n = decode_payload_len(first, 2)?;
        let mut bytes = [0u8; 2];
        crate::io::read_exact_bytes(reader, &mut bytes[..n])?;
        validate_canonical_payload(&bytes[..n])?;
        Ok(u16::from_le_bytes(bytes))
    }

    #[inline(always)]
    pub(crate) fn decode_varint_u32(reader: &mut impl Read) -> Result<u32> {
        if let Some(slice) = reader.buf() {
            let first = *slice.first().ok_or(Error::ReaderOutOfData)?;
            if first & 0x80 == 0 {
                reader.advance(1);
                return Ok(u32::from(first));
            }
            let n = decode_payload_len(first, 4)?;
            let payload = slice.get(1..1 + n).ok_or(Error::ReaderOutOfData)?;
            validate_canonical_payload(payload)?;

            let value = if slice.len() >= 5 {
                let raw =
                    u32::from_le(unsafe { (slice.as_ptr().add(1) as *const u32).read_unaligned() });
                if n < 4 {
                    raw & ((1u32 << (n << 3)) - 1)
                } else {
                    raw
                }
            } else {
                let mut bytes = [0u8; 4];
                bytes[..n].copy_from_slice(payload);
                u32::from_le_bytes(bytes)
            };
            reader.advance(1 + n);
            return Ok(value);
        }

        let mut first = [0u8; 1];
        crate::io::read_exact_bytes(reader, &mut first)?;
        let first = first[0];
        if first & 0x80 == 0 {
            return Ok(u32::from(first));
        }
        let n = decode_payload_len(first, 4)?;
        let mut bytes = [0u8; 4];
        crate::io::read_exact_bytes(reader, &mut bytes[..n])?;
        validate_canonical_payload(&bytes[..n])?;
        Ok(u32::from_le_bytes(bytes))
    }

    #[inline(always)]
    pub(crate) fn decode_varint_u64(reader: &mut impl Read) -> Result<u64> {
        if let Some(slice) = reader.buf() {
            let first = *slice.first().ok_or(Error::ReaderOutOfData)?;
            if first & 0x80 == 0 {
                reader.advance(1);
                return Ok(u64::from(first));
            }
            let n = decode_payload_len(first, 8)?;
            let payload = slice.get(1..1 + n).ok_or(Error::ReaderOutOfData)?;
            validate_canonical_payload(payload)?;

            let value = if slice.len() >= 9 {
                let raw =
                    u64::from_le(unsafe { (slice.as_ptr().add(1) as *const u64).read_unaligned() });
                if n < 8 {
                    raw & ((1u64 << (n << 3)) - 1)
                } else {
                    raw
                }
            } else {
                let mut bytes = [0u8; 8];
                bytes[..n].copy_from_slice(payload);
                u64::from_le_bytes(bytes)
            };
            reader.advance(1 + n);
            return Ok(value);
        }

        let mut first = [0u8; 1];
        crate::io::read_exact_bytes(reader, &mut first)?;
        let first = first[0];
        if first & 0x80 == 0 {
            return Ok(u64::from(first));
        }
        let n = decode_payload_len(first, 8)?;
        let mut bytes = [0u8; 8];
        crate::io::read_exact_bytes(reader, &mut bytes[..n])?;
        validate_canonical_payload(&bytes[..n])?;
        Ok(u64::from_le_bytes(bytes))
    }

    #[inline(always)]
    pub(crate) fn decode_varint_u128(reader: &mut impl Read) -> Result<u128> {
        if let Some(slice) = reader.buf() {
            let first = *slice.first().ok_or(Error::ReaderOutOfData)?;
            if first & 0x80 == 0 {
                reader.advance(1);
                return Ok(u128::from(first));
            }
            let n = decode_payload_len(first, 16)?;
            let payload = slice.get(1..1 + n).ok_or(Error::ReaderOutOfData)?;
            validate_canonical_payload(payload)?;

            let value = if slice.len() >= 17 {
                let ptr = unsafe { slice.as_ptr().add(1) };
                let lo = unsafe { u64::from_le((ptr as *const u64).read_unaligned()) };
                let hi = unsafe { u64::from_le((ptr.add(8) as *const u64).read_unaligned()) };
                // Mask using u64 ops instead of u128 shifts. For the hot
                // path (n=16, ~99.6% of random u128), no masking at all.
                if n >= 16 {
                    (lo as u128) | ((hi as u128) << 64)
                } else if n <= 8 {
                    let lo_masked = if n < 8 {
                        lo & ((1u64 << (n << 3)) - 1)
                    } else {
                        lo
                    };
                    lo_masked as u128
                } else {
                    let hi_bytes = n - 8;
                    let hi_masked = hi & ((1u64 << (hi_bytes << 3)) - 1);
                    (lo as u128) | ((hi_masked as u128) << 64)
                }
            } else {
                let mut bytes = [0u8; 16];
                bytes[..n].copy_from_slice(payload);
                u128::from_le_bytes(bytes)
            };
            reader.advance(1 + n);
            return Ok(value);
        }

        let mut first = [0u8; 1];
        crate::io::read_exact_bytes(reader, &mut first)?;
        let first = first[0];
        if first & 0x80 == 0 {
            return Ok(u128::from(first));
        }
        let n = decode_payload_len(first, 16)?;
        let mut bytes = [0u8; 16];
        crate::io::read_exact_bytes(reader, &mut bytes[..n])?;
        validate_canonical_payload(&bytes[..n])?;
        Ok(u128::from_le_bytes(bytes))
    }
}

impl VarintEncodingScheme for Lencode {
    #[inline(always)]
    fn encode_varint<I: UnsignedInteger>(val: I, writer: &mut impl Write) -> Result<usize> {
        // Zero-copy fast path
        if let Some(dst) = writer.buf_mut() {
            if (val >> 7) == I::ZERO {
                if dst.is_empty() {
                    return Err(Error::WriterOutOfSpace);
                }
                unsafe { *dst.get_unchecked_mut(0) = val.le_bytes()[0] };
                writer.advance_mut(1);
                return Ok(1);
            }

            let bytes = val.le_bytes();
            let bytes = bytes.as_slice();
            let mut n = bytes.len();
            while n > 1 && unsafe { *bytes.get_unchecked(n - 1) } == 0 {
                n -= 1;
            }

            let total = 1 + n;
            if dst.len() < total {
                return Err(Error::WriterOutOfSpace);
            }
            unsafe {
                *dst.get_unchecked_mut(0) = 0x80 | (n as u8 & 0x7F);
                core::ptr::copy_nonoverlapping(bytes.as_ptr(), dst.as_mut_ptr().add(1), n);
            }
            writer.advance_mut(total);
            return Ok(total);
        }

        // Fallback: write through trait
        if (val >> 7) == I::ZERO {
            let byte = val.le_bytes()[0];
            writer.write(core::slice::from_ref(&byte))?;
            return Ok(1);
        }

        let bytes = val.le_bytes();
        let bytes = bytes.as_slice();
        let mut n = bytes.len();
        while n > 1 && unsafe { *bytes.get_unchecked(n - 1) } == 0 {
            n -= 1;
        }

        let first_byte = 0x80 | (n as u8 & 0x7F);
        const STACK_BUF_BYTES: usize = 33; // 1-byte prefix + up to 32-byte payload (U256)
        if n < STACK_BUF_BYTES {
            let mut out = [0u8; STACK_BUF_BYTES];
            out[0] = first_byte;
            unsafe {
                core::ptr::copy_nonoverlapping(bytes.as_ptr(), out.as_mut_ptr().add(1), n);
            }
            writer.write(&out[..(1 + n)])?;
            Ok(1 + n)
        } else {
            writer.write(core::slice::from_ref(&first_byte))?;
            writer.write(&bytes[..n])?;
            Ok(1 + n)
        }
    }

    #[inline(always)]
    fn decode_varint<I: UnsignedInteger>(reader: &mut impl Read) -> Result<I> {
        let max_bytes = core::mem::size_of::<I>();
        if max_bytes == 0 {
            return Err(Error::InvalidData);
        }

        if let Some(slice) = reader.buf() {
            let first = *slice.first().ok_or(Error::ReaderOutOfData)?;
            if first & 0x80 == 0 {
                reader.advance(1);
                #[cfg(target_endian = "little")]
                {
                    let mut val = I::ZERO;
                    unsafe { *(&mut val as *mut I as *mut u8) = first };
                    return Ok(val);
                }
                #[cfg(target_endian = "big")]
                {
                    return Ok(from_le_bytes::<I>(&[first]));
                }
            }
            let n = decode_payload_len(first, max_bytes)?;
            let payload = slice.get(1..1 + n).ok_or(Error::ReaderOutOfData)?;
            validate_canonical_payload(payload)?;

            #[cfg(target_endian = "little")]
            {
                let mut val = I::ZERO;
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        slice.as_ptr().add(1),
                        &mut val as *mut I as *mut u8,
                        n,
                    );
                }
                reader.advance(1 + n);
                return Ok(val);
            }
            #[cfg(target_endian = "big")]
            {
                let value = from_le_bytes::<I>(payload);
                reader.advance(1 + n);
                return Ok(value);
            }
        }

        let mut first = [0u8; 1];
        crate::io::read_exact_bytes(reader, &mut first)?;
        let first = first[0];

        #[cfg(target_endian = "little")]
        {
            let mut val: I = I::ZERO;
            let val_bytes = unsafe {
                core::slice::from_raw_parts_mut(
                    &mut val as *mut I as *mut u8,
                    core::mem::size_of::<I>(),
                )
            };
            if first & 0x80 == 0 {
                val_bytes[0] = first;
                return Ok(val);
            }
            let n = decode_payload_len(first, max_bytes)?;
            crate::io::read_exact_bytes(reader, &mut val_bytes[..n])?;
            validate_canonical_payload(&val_bytes[..n])?;
            Ok(val)
        }

        #[cfg(target_endian = "big")]
        {
            if first & 0x80 == 0 {
                return Ok(from_le_bytes::<I>(&[first]));
            }
            let n = decode_payload_len(first, max_bytes)?;
            let mut bytes = I::ZERO.le_bytes();
            crate::io::read_exact_bytes(reader, &mut bytes[..n])?;
            validate_canonical_payload(&bytes[..n])?;
            return Ok(from_le_bytes::<I>(&bytes[..n]));
        }
    }

    #[inline(always)]
    fn encode_bool(val: bool, writer: &mut impl Write) -> Result<usize> {
        let byte = val as u8;
        if let Some(dst) = writer.buf_mut() {
            if dst.is_empty() {
                return Err(Error::WriterOutOfSpace);
            }
            unsafe { *dst.get_unchecked_mut(0) = byte };
            writer.advance_mut(1);
            return Ok(1);
        }
        writer.write(core::slice::from_ref(&byte))
    }

    #[inline(always)]
    fn decode_bool(reader: &mut impl Read) -> Result<bool> {
        if let Some(slice) = reader.buf() {
            if slice.is_empty() {
                return Err(Error::ReaderOutOfData);
            }
            let byte = unsafe { *slice.get_unchecked(0) };
            reader.advance(1);
            if byte > 1 {
                return Err(Error::InvalidData);
            }
            return Ok(byte != 0);
        }
        let mut byte = 0u8;
        crate::io::read_exact_bytes(reader, core::slice::from_mut(&mut byte))?;
        if byte > 1 {
            return Err(Error::InvalidData);
        }
        Ok(byte != 0)
    }
}

// when using lencode with u8 we bypass the integer encoding scheme so we don't waste bytes
impl Encode for u8 {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        _ctx: Option<&mut crate::context::EncoderContext>,
    ) -> Result<usize> {
        if let Some(dst) = writer.buf_mut() {
            if dst.is_empty() {
                return Err(Error::WriterOutOfSpace);
            }
            unsafe { *dst.get_unchecked_mut(0) = *self };
            writer.advance_mut(1);
            return Ok(1);
        }
        writer.write(core::slice::from_ref(self))
    }
}

impl Decode for u8 {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        _ctx: Option<&mut crate::context::DecoderContext>,
    ) -> Result<Self> {
        if let Some(slice) = reader.buf() {
            if slice.is_empty() {
                return Err(Error::ReaderOutOfData);
            }
            let byte = unsafe { *slice.get_unchecked(0) };
            reader.advance(1);
            return Ok(byte);
        }
        let mut buf = [0u8; 1];
        crate::io::read_exact_bytes(reader, &mut buf)?;
        Ok(buf[0])
    }
}

// when using lencode with i8 we bypass the integer encoding scheme so we don't waste bytes
impl Encode for i8 {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        _ctx: Option<&mut crate::context::EncoderContext>,
    ) -> Result<usize> {
        if let Some(dst) = writer.buf_mut() {
            if dst.is_empty() {
                return Err(Error::WriterOutOfSpace);
            }
            unsafe { *dst.get_unchecked_mut(0) = *self as u8 };
            writer.advance_mut(1);
            return Ok(1);
        }
        writer.write(&[*self as u8])
    }
}

impl Decode for i8 {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        _ctx: Option<&mut crate::context::DecoderContext>,
    ) -> Result<Self> {
        if let Some(slice) = reader.buf() {
            if slice.is_empty() {
                return Err(Error::ReaderOutOfData);
            }
            let byte = unsafe { *slice.get_unchecked(0) };
            reader.advance(1);
            return Ok(byte as i8);
        }
        let mut buf = [0u8; 1];
        crate::io::read_exact_bytes(reader, &mut buf)?;
        Ok(buf[0] as i8)
    }
}

#[test]
fn test_lencode_u8_small() {
    let mut buf = [0u8; 1];
    for i in 0..=127 {
        let val: u8 = i;
        let n = Lencode::encode_varint(val, &mut Cursor::new(&mut buf[..])).unwrap();
        assert_eq!(n, 1);
        let decoded = Lencode::decode_varint::<u8>(&mut Cursor::new(&buf)).unwrap();
        assert_eq!(decoded, val);
        assert_eq!(buf[0], val);
    }
}

#[test]
fn test_lencode_u8_large() {
    let mut buf = [0u8; 2];
    for i in 128..=255 {
        let val: u8 = i;
        let n = Lencode::encode_varint(val, &mut Cursor::new(&mut buf[..])).unwrap();
        assert_eq!(n, 2);
        let decoded = Lencode::decode_varint::<u8>(&mut Cursor::new(&buf)).unwrap();
        assert_eq!(decoded, val);
        assert_eq!(buf[0], 0x80 | 1);
        assert_eq!(buf[1], val);
    }
}

#[test]
fn test_lencode_u32_all() {
    let mut buf = [0u8; 5];
    for i in (0..=u32::MAX)
        .step_by(61)
        .chain(0..10000)
        .chain((u32::MAX - 10000)..=u32::MAX)
    {
        let val: u32 = i;
        let n = Lencode::encode_varint(val, &mut Cursor::new(&mut buf[..])).unwrap();
        let decoded = Lencode::decode_varint::<u32>(&mut Cursor::new(&buf[..n])).unwrap();
        if decoded != val {
            panic!(
                "FAIL: val={} buf={:02x?} decoded={} (size={})",
                val,
                &buf[..n],
                decoded,
                n
            );
        }
        assert_eq!(decoded, val);
    }
}

#[test]
fn test_lencode_u16_all() {
    let mut buf = [0u8; 3];
    for i in 0..=u16::MAX {
        let val: u16 = i;
        let n = Lencode::encode_varint(val, &mut Cursor::new(&mut buf[..])).unwrap();
        let decoded = Lencode::decode_varint::<u16>(&mut Cursor::new(&buf[..n])).unwrap();
        if decoded != val {
            panic!(
                "FAIL: val={} buf={:02x?} decoded={} (size={})",
                val,
                &buf[..n],
                decoded,
                n
            );
        }
        assert_eq!(decoded, val);
    }
}

#[test]
fn test_lencode_u64_all() {
    let mut buf = [0u8; const { 1 + mem::size_of::<u64>() }];
    for i in (0u32..u32::MAX)
        .step_by(30)
        .map(|x| (x as u64) << 32)
        .chain(0..10000)
        .chain((u64::MAX - 10000)..=u64::MAX)
    {
        let val: u64 = i;
        let n = Lencode::encode_varint(val, &mut Cursor::new(&mut buf[..])).unwrap();
        let decoded = Lencode::decode_varint::<u64>(&mut Cursor::new(&buf[..n])).unwrap();
        if decoded != val {
            panic!(
                "FAIL: val={} buf={:02x?} decoded={} (size={})",
                val,
                &buf[..n],
                decoded,
                n
            );
        }
        assert_eq!(decoded, val);
    }
}

#[test]
fn test_lencode_u128_small_values() {
    for i in 0..=127 {
        let val: u128 = i;
        let mut buf = [0u8; 1];
        let n = Lencode::encode_varint(val, &mut Cursor::new(&mut buf[..])).unwrap();
        assert_eq!(n, 1);
        let decoded = Lencode::decode_varint::<u128>(&mut Cursor::new(&buf)).unwrap();
        assert_eq!(decoded, val);
        assert_eq!(buf[0], val as u8);
    }
}

#[test]
fn test_lencode_u128_medium_values() {
    for i in 128..=255 {
        let val: u128 = i;
        let mut buf = [0u8; 2];
        let n = Lencode::encode_varint(val, &mut Cursor::new(&mut buf[..])).unwrap();
        assert_eq!(n, 2);
        let decoded = Lencode::decode_varint::<u128>(&mut Cursor::new(&buf)).unwrap();
        assert_eq!(decoded, val);
        assert_eq!(buf[0], 0x80 | 1);
        assert_eq!(buf[1], val as u8);
    }
}

#[test]
fn test_lencode_u128_multi_byte_values() {
    let mut buf = [0u8; 4];
    for i in 256..=1_000_000 {
        let val: u128 = i;
        let n = Lencode::encode_varint(val, &mut Cursor::new(&mut buf[..])).unwrap();
        let decoded = Lencode::decode_varint::<u128>(&mut Cursor::new(&buf[..n])).unwrap();
        if decoded != val {
            panic!(
                "FAIL: val={} buf={:02x?} decoded={} (size={})",
                val,
                &buf[..n],
                decoded,
                n
            );
        }
        assert_eq!(decoded, val);
    }
}

#[test]
fn test_encode_decode_lencode_u8_all() {
    for i in 0..=255 {
        let val: u8 = i;
        let mut buf = [0u8; 1];
        let n = u8::encode_ext(&val, &mut Cursor::new(&mut buf[..]), None).unwrap();
        assert_eq!(n, 1);
        let decoded = u8::decode_ext(&mut Cursor::new(&buf), None).unwrap();
        assert_eq!(decoded, val);
    }
}

#[test]
fn test_encode_decode_lencode_i8_all() {
    for i in -128..=127 {
        let val: i8 = i;
        let mut buf = [0u8; 1];
        let n = encode(&val, &mut Cursor::new(&mut buf[..])).unwrap();
        assert_eq!(n, 1);
        let decoded: i8 = decode(&mut Cursor::new(&buf)).unwrap();
        assert_eq!(decoded, val);
    }
}

#[test]
fn test_encode_decode_u256() {
    use crate::u256::U256;
    let mut buf = [0u8; 33];
    for i in 0..=1000u128 {
        let val: U256 = U256::from(i * i * i * i * i * i * i * i * i * i * i * i);
        let mut cursor = Cursor::new(&mut buf);
        let n = Lencode::encode_varint(val, &mut cursor).unwrap();
        assert!(
            n <= 16,
            "Encoded size should not exceed 16 bytes based on this range"
        );
        let decoded = Lencode::decode_varint::<U256>(&mut Cursor::new(&buf[..n])).unwrap();
        assert_eq!(decoded, val, "Failed for iteration {}", i);
    }
}

#[test]
fn specialized_native_varints_preserve_canonical_boundaries() {
    macro_rules! check {
        ($ty:ty, $encode:ident, $decode:ident, [$($value:expr),+ $(,)?]) => {
            $(
                let value = $value as $ty;
                let mut encoded = Vec::new();
                Lencode::$encode(value, &mut encoded).unwrap();

                let mut specialized = Cursor::new(encoded.as_slice());
                assert_eq!(Lencode::$decode(&mut specialized).unwrap(), value);
                assert_eq!(specialized.position(), encoded.len());

                let mut generic = Cursor::new(encoded.as_slice());
                assert_eq!(Lencode::decode_varint::<$ty>(&mut generic).unwrap(), value);
                assert_eq!(generic.position(), encoded.len());
            )+
        };
    }

    check!(
        u16,
        encode_varint_u16,
        decode_varint_u16,
        [0, 1, 127, 128, 255, 256, u16::MAX]
    );
    check!(
        u32,
        encode_varint_u32,
        decode_varint_u32,
        [0, 1, 127, 128, 255, 256, 65_535, 65_536, u32::MAX]
    );
    check!(
        u64,
        encode_varint_u64,
        decode_varint_u64,
        [
            0,
            1,
            127,
            128,
            255,
            256,
            65_535,
            65_536,
            1u64 << 32,
            u64::MAX,
        ]
    );
    check!(
        u128,
        encode_varint_u128,
        decode_varint_u128,
        [
            0,
            1,
            127,
            128,
            255,
            256,
            65_535,
            65_536,
            1u128 << 64,
            u128::MAX,
        ]
    );
}

#[cfg(test)]
fn malformed_native_varints(max_bytes: usize) -> Vec<Vec<u8>> {
    let mut oversized = vec![0x80 | (max_bytes as u8 + 1)];
    oversized.extend(core::iter::repeat_n(1, max_bytes + 1));
    let mut maximum_prefix = vec![0xff];
    maximum_prefix.extend(core::iter::repeat_n(1, 127));
    let mut truncated = vec![0x80 | max_bytes as u8];
    truncated.extend(core::iter::repeat_n(1, max_bytes.saturating_sub(1)));

    vec![
        Vec::new(),
        vec![0x80],          // Zero-length payload.
        vec![0x81],          // Truncated one-byte payload.
        vec![0x81, 0],       // Redundant high zero byte.
        vec![0x81, 1],       // Value belongs in the small form.
        vec![0x81, 127],     // Largest value belonging in small form.
        vec![0x82, 0x80, 0], // Redundant high zero byte.
        vec![0x82, 0xff, 0], // Another non-minimal two-byte value.
        oversized,
        maximum_prefix,
        truncated,
    ]
}

#[test]
fn every_specialized_and_generic_decoder_rejects_malformed_native_varints() {
    macro_rules! rejects {
        ($max_bytes:expr, $decode:expr) => {
            for input in malformed_native_varints($max_bytes) {
                let mut reader = Cursor::new(input.as_slice());
                assert!(($decode)(&mut reader).is_err(), "accepted {input:02x?}");
                assert_eq!(reader.position(), 0, "advanced for {input:02x?}");
            }
        };
    }

    rejects!(2, Lencode::decode_varint_u16);
    rejects!(4, Lencode::decode_varint_u32);
    rejects!(8, Lencode::decode_varint_u64);
    rejects!(16, Lencode::decode_varint_u128);
    rejects!(1, Lencode::decode_varint::<u8>);
    rejects!(2, Lencode::decode_varint::<u16>);
    rejects!(4, Lencode::decode_varint::<u32>);
    rejects!(8, Lencode::decode_varint::<u64>);
    rejects!(16, Lencode::decode_varint::<u128>);
    rejects!(32, Lencode::decode_varint::<crate::u256::U256>);

    // Signed native decoding delegates to the same hardened unsigned parser.
    rejects!(8, Lencode::decode_varint_signed::<i64>);
}

#[test]
fn every_native_length_prefix_is_bounded_and_canonical() {
    macro_rules! check_headers {
        ($max_bytes:expr, $decode:expr, $encode:expr) => {
            for first in 0x80u8..=0xff {
                let payload_len = usize::from(first & 0x7f);
                let mut input = vec![first];
                input.extend(core::iter::repeat_n(0x80, 127));
                if payload_len == 1 {
                    input[1] = 0x80;
                } else if payload_len > 1 {
                    input[payload_len] = 1;
                }

                let mut reader = Cursor::new(input.as_slice());
                if (1..=$max_bytes).contains(&payload_len) {
                    let value = ($decode)(&mut reader).unwrap();
                    assert_eq!(reader.position(), 1 + payload_len);
                    let mut canonical = Vec::new();
                    ($encode)(value, &mut canonical).unwrap();
                    assert_eq!(canonical, input[..reader.position()]);
                } else {
                    assert!(($decode)(&mut reader).is_err());
                    assert_eq!(reader.position(), 0);
                }
            }
        };
    }

    check_headers!(2, Lencode::decode_varint_u16, Lencode::encode_varint_u16);
    check_headers!(4, Lencode::decode_varint_u32, Lencode::encode_varint_u32);
    check_headers!(8, Lencode::decode_varint_u64, Lencode::encode_varint_u64);
    check_headers!(16, Lencode::decode_varint_u128, Lencode::encode_varint_u128);
    check_headers!(
        32,
        Lencode::decode_varint::<crate::u256::U256>,
        Lencode::encode_varint
    );
}

#[test]
fn arbitrary_native_inputs_only_decode_canonical_prefixes() {
    macro_rules! fuzz_like {
        ($decode:expr, $encode:expr) => {{
            let mut state = 0xd1b5_4a32_d192_ed03u64;
            for sample in 0..10_000usize {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let len = sample % 40;
                let mut input = vec![0u8; len];
                for byte in &mut input {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    *byte = state as u8;
                }

                let mut reader = Cursor::new(input.as_slice());
                match ($decode)(&mut reader) {
                    Ok(value) => {
                        let mut canonical = Vec::new();
                        ($encode)(value, &mut canonical).unwrap();
                        assert_eq!(canonical, input[..reader.position()]);
                    }
                    Err(_) => assert_eq!(reader.position(), 0),
                }
            }
        }};
    }

    fuzz_like!(Lencode::decode_varint_u16, Lencode::encode_varint_u16);
    fuzz_like!(Lencode::decode_varint_u32, Lencode::encode_varint_u32);
    fuzz_like!(Lencode::decode_varint_u64, Lencode::encode_varint_u64);
    fuzz_like!(Lencode::decode_varint_u128, Lencode::encode_varint_u128);
    fuzz_like!(
        Lencode::decode_varint::<crate::u256::U256>,
        Lencode::encode_varint
    );
    fuzz_like!(
        Lencode::decode_varint_signed::<i64>,
        Lencode::encode_varint_signed
    );
}

#[test]
fn streaming_native_decoders_handle_partial_reads_and_truncation() {
    struct OneByteReader<'a> {
        input: &'a [u8],
        position: usize,
    }

    impl Read for OneByteReader<'_> {
        fn read(&mut self, output: &mut [u8]) -> Result<usize> {
            if self.position == self.input.len() {
                return Ok(0);
            }
            if output.is_empty() {
                return Ok(0);
            }
            output[0] = self.input[self.position];
            self.position += 1;
            Ok(1)
        }
    }

    let mut encoded = Vec::new();
    Lencode::encode_varint_u128(u128::MAX, &mut encoded).unwrap();
    let mut reader = OneByteReader {
        input: &encoded,
        position: 0,
    };
    assert_eq!(Lencode::decode_varint_u128(&mut reader).unwrap(), u128::MAX);
    assert_eq!(reader.position, encoded.len());

    let mut truncated = OneByteReader {
        input: &encoded[..encoded.len() - 1],
        position: 0,
    };
    assert!(Lencode::decode_varint_u128(&mut truncated).is_err());

    let oversized = [0xff];
    let mut oversized = OneByteReader {
        input: &oversized,
        position: 0,
    };
    assert!(Lencode::decode_varint_u64(&mut oversized).is_err());
    assert_eq!(oversized.position, 1, "payload must not be requested");
}
