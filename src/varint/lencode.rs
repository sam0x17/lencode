use crate::prelude::*;
use core::mem;

/// The Lencode integer encoding [`Scheme`] is designed to encode integers in a variable-length
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

#[inline(always)]
pub fn encode<T: Encode>(value: &T, writer: &mut impl Write) -> Result<usize> {
    value.encode(writer)
}

#[inline(always)]
pub fn decode<T: Decode>(reader: &mut impl Read) -> Result<T> {
    T::decode(reader)
}

impl Scheme for Lencode {
    #[inline(always)]
    fn encode_varint<I: UnsignedInteger>(val: I, writer: &mut impl Write) -> Result<usize> {
        let mask = I::MAX_VALUE - I::ONE_HUNDRED_TWENTY_SEVEN;
        if (val & mask) == I::ZERO {
            let byte = val.le_bytes()[0];
            writer.write(&[byte])?;
            return Ok(1);
        }

        let le_bytes = val.le_bytes();
        let mut n = le_bytes.len();
        while n > 0 && le_bytes[n - 1] == 0 {
            n -= 1;
        }

        let first_byte = 0x80 | (n as u8 & 0x7F);
        writer.write(&[first_byte])?;
        writer.write(&le_bytes[..n])?;
        Ok(1 + n)
    }

    #[inline(always)]
    fn decode_varint<I: UnsignedInteger>(reader: &mut impl Read) -> Result<I> {
        let mut val: I = I::ZERO;
        let val_bytes = unsafe {
            core::slice::from_raw_parts_mut(&mut val as *mut I as *mut u8, mem::size_of::<I>())
        };
        reader.read(&mut val_bytes[..1])?;
        let first_byte = val_bytes[0];

        if first_byte & 0x80 == 0 {
            // Small integer: single byte, left-align in buffer (little endian)
            val_bytes[0] = first_byte & 0x7F;
            Ok(val)
        } else {
            // Large integer: read n bytes, left-align in buffer (little endian)
            let n = (first_byte & 0x7F) as usize;
            // if n == 0 || n > mem::size_of::<I>() {
            //     return Err(Error::InvalidData);
            // }
            reader.read(&mut val_bytes[..n])?;
            #[cfg(target_endian = "big")]
            reverse(val_bytes);
            Ok(val)
        }
    }

    #[inline(always)]
    fn encode_bool(val: bool, writer: &mut impl Write) -> Result<usize> {
        writer.write(&[if val { 1u8 } else { 0u8 }])
    }

    #[inline(always)]
    fn decode_bool(reader: &mut impl Read) -> Result<bool> {
        let mut byte = 0u8;
        reader.read(core::slice::from_mut(&mut byte))?;
        match byte {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::InvalidData),
        }
    }
}

// when using lencode with u8 we bypass the integer encoding scheme so we don't waste bytes
impl Encode for u8 {
    #[inline(always)]
    fn encode(&self, writer: &mut impl Write) -> Result<usize> {
        writer.write(&[*self])
    }
}

impl Decode for u8 {
    #[inline(always)]
    fn decode(reader: &mut impl Read) -> Result<Self> {
        let mut buf = [0u8; 1];
        reader.read(&mut buf)?;
        Ok(buf[0])
    }
}

// when using lencode with i8 we bypass the integer encoding scheme so we don't waste bytes
impl Encode for i8 {
    #[inline(always)]
    fn encode(&self, writer: &mut impl Write) -> Result<usize> {
        writer.write(&[*self as u8])
    }
}

impl Decode for i8 {
    #[inline(always)]
    fn decode(reader: &mut impl Read) -> Result<Self> {
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
        let n = u8::encode(&val, &mut Cursor::new(&mut buf[..])).unwrap();
        assert_eq!(n, 1);
        let decoded = u8::decode(&mut Cursor::new(&buf)).unwrap();
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
