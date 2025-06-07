use crate::prelude::*;
use core::mem;
use core::ptr;

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

// Helper: reconstruct integer from big-endian bytes
fn int_from_be_bytes<I: UnsignedInteger>(be_bytes: &[u8]) -> I {
    let mut val: I = unsafe { mem::zeroed() };
    let size = mem::size_of::<I>();
    unsafe {
        ptr::copy_nonoverlapping(be_bytes.as_ptr(), &mut val as *mut I as *mut u8, size);
    }
    val
}

impl Scheme for Lencode {
    fn encode<I: UnsignedInteger>(val: I, mut writer: impl Write) -> Result<usize> {
        let be_bytes = val.be_bytes();
        let size = be_bytes.len();
        // Strip leading zeros for minimal encoding
        let first_nonzero = be_bytes.iter().position(|&b| b != 0).unwrap_or(size - 1);
        let minimal = &be_bytes[first_nonzero..];
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

    fn decode<I: UnsignedInteger>(mut reader: impl Read) -> Result<I> {
        let mut first = [0u8; 1];
        reader.read(&mut first)?;
        let first_byte = first[0];
        let size = mem::size_of::<I>();
        let mut arr = [0u8; 16];
        let be_bytes = &mut arr[16 - size..];
        if first_byte & 0x80 == 0 {
            // Small integer
            be_bytes[size - 1] = first_byte & 0x7F;
        } else {
            // Large integer
            let n = (first_byte & 0x7F) as usize;
            if n == 0 || n > size {
                return Err(Error::InvalidData);
            }
            reader.read(&mut be_bytes[size - n..])?;
        }
        Ok(int_from_be_bytes::<I>(be_bytes))
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
    for i in 0..=u32::MAX {
        let val: u32 = i;
        let mut buf = [0u8; 5];
        let _ = Lencode::encode(val, Cursor::new(&mut buf[..])).unwrap();
        let decoded = Lencode::decode::<u32>(Cursor::new(&buf[..])).unwrap();
        assert_eq!(decoded, val);
    }
}
