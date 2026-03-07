use crate::prelude::*;
#[cfg(test)]
use core::mem;

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
        // Zero-copy fast path
        if let Some(dst) = writer.buf_mut() {
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
            unsafe {
                *dst.get_unchecked_mut(0) = 0x80 | (n as u8);
                if dst.len() >= 3 {
                    let le = val.to_le_bytes();
                    (dst.as_mut_ptr().add(1) as *mut [u8; 2]).write_unaligned(le);
                } else {
                    core::ptr::copy_nonoverlapping(
                        &val as *const u16 as *const u8,
                        dst.as_mut_ptr().add(1),
                        n,
                    );
                }
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
        // Zero-copy fast path
        if let Some(dst) = writer.buf_mut() {
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
            unsafe {
                *dst.get_unchecked_mut(0) = 0x80 | (n as u8);
                if dst.len() >= 5 {
                    let le = val.to_le_bytes();
                    (dst.as_mut_ptr().add(1) as *mut [u8; 4]).write_unaligned(le);
                } else {
                    core::ptr::copy_nonoverlapping(
                        &val as *const u32 as *const u8,
                        dst.as_mut_ptr().add(1),
                        n,
                    );
                }
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
        // Zero-copy fast path
        if let Some(dst) = writer.buf_mut() {
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
            unsafe {
                *dst.get_unchecked_mut(0) = 0x80 | (n as u8);
                if dst.len() >= 9 {
                    // Fast: write all 8 LE bytes at once (avoids variable-length copy).
                    let le = val.to_le_bytes();
                    (dst.as_mut_ptr().add(1) as *mut [u8; 8]).write_unaligned(le);
                } else {
                    core::ptr::copy_nonoverlapping(
                        &val as *const u64 as *const u8,
                        dst.as_mut_ptr().add(1),
                        n,
                    );
                }
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
        // Zero-copy fast path
        if let Some(dst) = writer.buf_mut() {
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
            unsafe {
                *dst.get_unchecked_mut(0) = 0x80 | (n as u8);
                if dst.len() >= 17 {
                    let le = val.to_le_bytes();
                    (dst.as_mut_ptr().add(1) as *mut [u8; 16]).write_unaligned(le);
                } else {
                    core::ptr::copy_nonoverlapping(
                        &val as *const u128 as *const u8,
                        dst.as_mut_ptr().add(1),
                        n,
                    );
                }
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
        // Zero-copy fast path
        if let Some(slice) = reader.buf() {
            if slice.is_empty() {
                return Err(Error::ReaderOutOfData);
            }
            let first = unsafe { *slice.get_unchecked(0) };
            if first & 0x80 == 0 {
                reader.advance(1);
                return Ok(first as u16);
            }
            let n = (first & 0x7F) as usize;
            if 1 + n > slice.len() {
                return Err(Error::ReaderOutOfData);
            }
            let val: u16 = if slice.len() >= 3 {
                let raw = unsafe { (slice.as_ptr().add(1) as *const u16).read_unaligned() };
                if n < 2 {
                    raw & ((1u16 << (n << 3)) - 1)
                } else {
                    raw
                }
            } else {
                let mut v: u16 = 0;
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        slice.as_ptr().add(1),
                        &mut v as *mut u16 as *mut u8,
                        n,
                    );
                }
                v
            };
            reader.advance(1 + n);
            return Ok(val);
        }
        // Fallback
        let mut first = 0u8;
        reader.read(core::slice::from_mut(&mut first))?;
        if first & 0x80 == 0 {
            return Ok(first as u16);
        }
        let n = (first & 0x7F) as usize;
        let mut val: u16 = 0;
        let val_bytes =
            unsafe { core::slice::from_raw_parts_mut(&mut val as *mut u16 as *mut u8, 2) };
        reader.read(&mut val_bytes[..n])?;
        Ok(val)
    }

    #[inline(always)]
    pub(crate) fn decode_varint_u32(reader: &mut impl Read) -> Result<u32> {
        // Zero-copy fast path
        if let Some(slice) = reader.buf() {
            if slice.is_empty() {
                return Err(Error::ReaderOutOfData);
            }
            let first = unsafe { *slice.get_unchecked(0) };
            if first & 0x80 == 0 {
                reader.advance(1);
                return Ok(first as u32);
            }
            let n = (first & 0x7F) as usize;
            if 1 + n > slice.len() {
                return Err(Error::ReaderOutOfData);
            }
            let val: u32 = if slice.len() >= 5 {
                let raw = unsafe { (slice.as_ptr().add(1) as *const u32).read_unaligned() };
                if n < 4 {
                    raw & ((1u32 << (n << 3)) - 1)
                } else {
                    raw
                }
            } else {
                let mut v: u32 = 0;
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        slice.as_ptr().add(1),
                        &mut v as *mut u32 as *mut u8,
                        n,
                    );
                }
                v
            };
            reader.advance(1 + n);
            return Ok(val);
        }
        // Fallback
        let mut first = 0u8;
        reader.read(core::slice::from_mut(&mut first))?;
        if first & 0x80 == 0 {
            return Ok(first as u32);
        }
        let n = (first & 0x7F) as usize;
        let mut val: u32 = 0;
        let val_bytes =
            unsafe { core::slice::from_raw_parts_mut(&mut val as *mut u32 as *mut u8, 4) };
        reader.read(&mut val_bytes[..n])?;
        Ok(val)
    }

    #[inline(always)]
    pub(crate) fn decode_varint_u64(reader: &mut impl Read) -> Result<u64> {
        // Zero-copy fast path
        if let Some(slice) = reader.buf() {
            if slice.is_empty() {
                return Err(Error::ReaderOutOfData);
            }
            let first = unsafe { *slice.get_unchecked(0) };
            if first & 0x80 == 0 {
                reader.advance(1);
                return Ok(first as u64);
            }
            let n = (first & 0x7F) as usize;
            if 1 + n > slice.len() {
                return Err(Error::ReaderOutOfData);
            }
            let val: u64 = if slice.len() >= 9 {
                // Fast: read full 8 bytes and mask to n bytes
                let raw = unsafe { (slice.as_ptr().add(1) as *const u64).read_unaligned() };
                if n < 8 {
                    raw & ((1u64 << (n << 3)) - 1)
                } else {
                    raw
                }
            } else {
                let mut v: u64 = 0;
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        slice.as_ptr().add(1),
                        &mut v as *mut u64 as *mut u8,
                        n,
                    );
                }
                v
            };
            reader.advance(1 + n);
            return Ok(val);
        }
        // Fallback: 2-read path
        let mut first = 0u8;
        reader.read(core::slice::from_mut(&mut first))?;
        if first & 0x80 == 0 {
            return Ok(first as u64);
        }
        let n = (first & 0x7F) as usize;
        #[cfg(target_endian = "little")]
        {
            let mut val: u64 = 0;
            let val_bytes =
                unsafe { core::slice::from_raw_parts_mut(&mut val as *mut u64 as *mut u8, 8) };
            reader.read(&mut val_bytes[..n])?;
            Ok(val)
        }
        #[cfg(target_endian = "big")]
        {
            let mut buf = [0u8; 8];
            reader.read(&mut buf[..n])?;
            let mut val = 0u64;
            let mut shift = 0u32;
            for i in 0..n {
                val |= (buf[i] as u64) << shift;
                shift += 8;
            }
            Ok(val)
        }
    }

    #[inline(always)]
    pub(crate) fn decode_varint_u128(reader: &mut impl Read) -> Result<u128> {
        // Zero-copy fast path
        if let Some(slice) = reader.buf() {
            if slice.is_empty() {
                return Err(Error::ReaderOutOfData);
            }
            let first = unsafe { *slice.get_unchecked(0) };
            if first & 0x80 == 0 {
                reader.advance(1);
                return Ok(first as u128);
            }
            let n = (first & 0x7F) as usize;
            if 1 + n > slice.len() {
                return Err(Error::ReaderOutOfData);
            }
            let val: u128 = if slice.len() >= 17 {
                let raw = unsafe { (slice.as_ptr().add(1) as *const u128).read_unaligned() };
                if n < 16 {
                    raw & (!0u128 >> ((16 - n) << 3))
                } else {
                    raw
                }
            } else {
                let mut v: u128 = 0;
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        slice.as_ptr().add(1),
                        &mut v as *mut u128 as *mut u8,
                        n,
                    );
                }
                v
            };
            reader.advance(1 + n);
            return Ok(val);
        }
        // Fallback: 2-read path
        let mut first = 0u8;
        reader.read(core::slice::from_mut(&mut first))?;
        if first & 0x80 == 0 {
            return Ok(first as u128);
        }
        let n = (first & 0x7F) as usize;
        #[cfg(target_endian = "little")]
        {
            let mut val: u128 = 0;
            let val_bytes =
                unsafe { core::slice::from_raw_parts_mut(&mut val as *mut u128 as *mut u8, 16) };
            reader.read(&mut val_bytes[..n])?;
            Ok(val)
        }
        #[cfg(target_endian = "big")]
        {
            let mut buf = [0u8; 16];
            reader.read(&mut buf[..n])?;
            let mut val = 0u128;
            let mut shift = 0u32;
            for i in 0..n {
                val |= (buf[i] as u128) << shift;
                shift += 8;
            }
            Ok(val)
        }
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
                #[cfg(target_endian = "little")]
                let byte = val.ne_bytes()[0];
                #[cfg(target_endian = "big")]
                let byte = val.le_bytes()[0];
                unsafe { *dst.get_unchecked_mut(0) = byte };
                writer.advance_mut(1);
                return Ok(1);
            }

            #[cfg(target_endian = "little")]
            let bytes = val.ne_bytes();
            #[cfg(target_endian = "big")]
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
            #[cfg(target_endian = "little")]
            let byte = val.ne_bytes()[0];
            #[cfg(target_endian = "big")]
            let byte = val.le_bytes()[0];
            writer.write(core::slice::from_ref(&byte))?;
            return Ok(1);
        }

        #[cfg(target_endian = "little")]
        let bytes = val.ne_bytes();
        #[cfg(target_endian = "big")]
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
        // Zero-copy fast path
        if let Some(slice) = reader.buf() {
            if slice.is_empty() {
                return Err(Error::ReaderOutOfData);
            }
            let first = unsafe { *slice.get_unchecked(0) };
            if first & 0x80 == 0 {
                reader.advance(1);
                // Build value from the single byte
                let mut val = I::ZERO;
                let val_bytes = unsafe {
                    core::slice::from_raw_parts_mut(
                        &mut val as *mut I as *mut u8,
                        core::mem::size_of::<I>(),
                    )
                };
                unsafe { *val_bytes.get_unchecked_mut(0) = first };
                return Ok(val);
            }
            let n = (first & 0x7F) as usize;
            if 1 + n > slice.len() {
                return Err(Error::ReaderOutOfData);
            }
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
                let mut buf = [0u8; 32];
                unsafe {
                    core::ptr::copy_nonoverlapping(slice.as_ptr().add(1), buf.as_mut_ptr(), n);
                }
                reader.advance(1 + n);
                let mut val = I::ZERO;
                let mut base = I::ONE;
                for i in 0..n {
                    let byte = buf[i];
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
                    if i + 1 < n {
                        base = base << 8;
                    }
                }
                return Ok(val);
            }
        }

        // Fallback: 2-read path
        #[cfg(target_endian = "little")]
        {
            let mut val: I = I::ZERO;
            let val_bytes = unsafe {
                core::slice::from_raw_parts_mut(
                    &mut val as *mut I as *mut u8,
                    core::mem::size_of::<I>(),
                )
            };
            reader.read(&mut val_bytes[..1])?;
            let first = unsafe { *val_bytes.get_unchecked(0) };
            if first & 0x80 == 0 {
                return Ok(val);
            }
            let n = (first & 0x7F) as usize;
            reader.read(&mut val_bytes[..n])?;
            Ok(val)
        }

        #[cfg(target_endian = "big")]
        {
            let mut first = 0u8;
            reader.read(core::slice::from_mut(&mut first))?;
            let mut buf = [0u8; 32];
            let n: usize;
            if first & 0x80 == 0 {
                buf[0] = first & 0x7F;
                n = 1;
            } else {
                n = (first & 0x7F) as usize;
                reader.read(&mut buf[..n])?;
            }

            let mut val = I::ZERO;
            let mut base = I::ONE;
            for i in 0..n {
                let byte = buf[i];
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
                if i + 1 < n {
                    base = base << 8;
                }
            }
            return Ok(val);
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
        reader.read(core::slice::from_mut(&mut byte))?;
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
        _dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
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
        _dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
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
        reader.read(&mut buf)?;
        Ok(buf[0])
    }
}

// when using lencode with i8 we bypass the integer encoding scheme so we don't waste bytes
impl Encode for i8 {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        _dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
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
        _dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
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
        reader.read(&mut buf)?;
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
