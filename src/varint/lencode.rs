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

// Helper: reconstruct integer from little-endian bytes
#[inline(always)]
fn int_from_le_bytes<I: UnsignedInteger>(le_bytes: &[u8]) -> I {
    let mut val: I = unsafe { core::mem::zeroed() };
    unsafe {
        core::ptr::copy_nonoverlapping(
            le_bytes.as_ptr(),
            &mut val as *mut I as *mut u8,
            le_bytes.len(),
        );
    }
    val
}

impl Scheme for Lencode {
    #[inline(always)]
    fn encode<I: UnsignedInteger>(val: I, mut writer: impl Write) -> Result<usize> {
        let le_bytes = val.le_bytes();
        // Strip trailing zeros for minimal encoding (little endian)
        let last_nonzero = le_bytes.iter().rposition(|&b| b != 0).unwrap_or(0);
        let minimal = &le_bytes[..=last_nonzero];
        if minimal.len() == 1 && minimal[0] <= 127 {
            writer.write(&[minimal[0]])?;
            return Ok(1);
        }
        let n = minimal.len();
        if n > 127 {
            return Err(Error::InvalidData);
        }
        let first_byte = 0x80 | (n as u8 & 0x7F);
        writer.write(&[first_byte])?;
        writer.write(minimal)?;
        Ok(1 + n)
    }

    #[inline(always)]
    fn decode<I: UnsignedInteger>(mut reader: impl Read) -> Result<I> {
        let mut first = [0u8; 1];
        reader.read(&mut first)?;
        let first_byte = first[0];
        let size = mem::size_of::<I>();
        let mut buf = [0u8; 16];
        if first_byte & 0x80 == 0 {
            // Small integer: single byte, left-align in buffer (little endian)
            buf[0] = first_byte & 0x7F;
            return Ok(int_from_le_bytes::<I>(&buf[..size]));
        } else {
            // Large integer: read n bytes, left-align in buffer (little endian)
            let n = (first_byte & 0x7F) as usize;
            if n == 0 || n > size {
                return Err(Error::InvalidData);
            }
            reader.read(&mut buf[..n])?;
            Ok(int_from_le_bytes::<I>(&buf[..size]))
        }
    }
}

#[test]
fn test_lencode_u8_small() {
    for i in 0..=127 {
        let val: u8 = i;
        let mut buf = [0u8; 1];
        let n = Lencode::encode(val, Cursor::new(&mut buf[..])).unwrap();
        assert_eq!(n, 1);
        let decoded = Lencode::decode::<u8>(Cursor::new(&buf)).unwrap();
        assert_eq!(decoded, val);
        assert_eq!(buf[0], val);
    }
}

#[test]
fn test_lencode_u8_large() {
    for i in 128..=255 {
        let val: u8 = i;
        let mut buf = [0u8; 2];
        let n = Lencode::encode(val, Cursor::new(&mut buf[..])).unwrap();
        assert_eq!(n, 2);
        let decoded = Lencode::decode::<u8>(Cursor::new(&buf)).unwrap();
        assert_eq!(decoded, val);
        assert_eq!(buf[0], 0x80 | 1);
        assert_eq!(buf[1], val);
    }
}

#[test]
fn test_lencode_u32_all() {
    for i in (0..=u32::MAX)
        .step_by(61)
        .chain(0..10000)
        .chain((u32::MAX - 10000)..=u32::MAX)
    {
        let val: u32 = i;
        let mut buf = [0u8; 5];
        let n = Lencode::encode(val, Cursor::new(&mut buf[..])).unwrap();
        let decoded = Lencode::decode::<u32>(Cursor::new(&buf[..n])).unwrap();
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
    for i in 0..=u16::MAX {
        let val: u16 = i;
        let mut buf = [0u8; 3];
        let n = Lencode::encode(val, Cursor::new(&mut buf[..])).unwrap();
        let decoded = Lencode::decode::<u16>(Cursor::new(&buf[..n])).unwrap();
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
    for i in (0..=u64::MAX)
        .step_by(91111111117)
        .chain(0..10000)
        .chain((u64::MAX - 10000)..=u64::MAX)
    {
        let val: u64 = i;
        let mut buf = [0u8; const { 1 + mem::size_of::<u64>() }];
        let n = Lencode::encode(val, Cursor::new(&mut buf[..])).unwrap();
        let decoded = Lencode::decode::<u64>(Cursor::new(&buf[..n])).unwrap();
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
        let n = Lencode::encode(val, Cursor::new(&mut buf[..])).unwrap();
        assert_eq!(n, 1);
        let decoded = Lencode::decode::<u128>(Cursor::new(&buf)).unwrap();
        assert_eq!(decoded, val);
        assert_eq!(buf[0], val as u8);
    }
}

#[test]
fn test_lencode_u128_medium_values() {
    for i in 128..=255 {
        let val: u128 = i;
        let mut buf = [0u8; 2];
        let n = Lencode::encode(val, Cursor::new(&mut buf[..])).unwrap();
        assert_eq!(n, 2);
        let decoded = Lencode::decode::<u128>(Cursor::new(&buf)).unwrap();
        assert_eq!(decoded, val);
        assert_eq!(buf[0], 0x80 | 1);
        assert_eq!(buf[1], val as u8);
    }
}

#[test]
fn test_lencode_u128_multi_byte_values() {
    for i in 256..=1_000_000 {
        let val: u128 = i;
        let mut buf = [0u8; 4];
        let n = Lencode::encode(val, Cursor::new(&mut buf[..])).unwrap();
        let decoded = Lencode::decode::<u128>(Cursor::new(&buf[..n])).unwrap();
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
