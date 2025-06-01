use endian_cast::Endianness;

use crate::io::{BitReader, BitWriter, Read, Write};
use crate::*;
use bitvec::prelude::*;

pub trait VarInt: Endianness + Default + Eq + core::fmt::Debug {
    /// Encodes the value into raw bits using the len4 encoding scheme.
    fn encode<W: Write, const N: usize>(self, _writer: &mut BitWriter<W, N>) -> Result<usize> {
        todo!()
    }

    /// Decodes the value from raw bits using the len4 encoding scheme.
    fn decode<R: Read, const N: usize>(reader: &mut BitReader<R, Msb0, N>) -> Result<Self> {
        let first_bit = reader.read_bit()?;
        let mut val = Self::default();
        let buf: &mut [u8] = unsafe {
            core::slice::from_raw_parts_mut(
                &mut val as *mut Self as *mut u8,
                core::mem::size_of::<Self>(),
            )
        };
        if first_bit {
            let mut bitsize: usize = 0;
            bitsize += 4 * reader.read_ones()?;
            bitsize += reader
                .read_zeros()?
                .checked_sub(1)
                .ok_or(Error::InvalidData)?;
            reader.read_one()?; // read sentinel bit
            if bitsize > core::mem::size_of::<Self>() * 8 {
                return Err(Error::InvalidData);
            }
            for i in 0..bitsize {
                let bit = reader.read_bit()?;
                // each bit we read is part of the binary representation of the value, i.e.
                // 0b10 is 2, ob11 is 3, etc., so we set each bit in the value accordingly
                let byte_index = i / 8;
                let bit_index = (bitsize - 1 - i) % 8;
                if bit {
                    buf[byte_index] |= 1 << bit_index;
                } else {
                    buf[byte_index] &= !(1 << bit_index);
                }
            }
        } // else the value is a zero
        // reverse byte order if we are big-endian
        #[cfg(target_endian = "big")]
        reverse(buf);
        Ok(val)
    }
}

impl VarInt for u8 {}
impl VarInt for u16 {}
impl VarInt for u32 {}
impl VarInt for u64 {}
impl VarInt for u128 {}
impl VarInt for usize {}

#[inline(always)]
pub const fn reverse(bytes: &mut [u8]) {
    let mut i = 0;
    let mut j = bytes.len() - 1;

    while i < j {
        let tmp = bytes[i];
        bytes[i] = bytes[j];
        bytes[j] = tmp;

        i += 1;
        j -= 1;
    }
}

#[test]
fn test_decode_varint_0() {
    let data = vec![0b0111_1111]; // pad with 1s to smoke test the zero case
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = VarInt::decode(&mut reader).unwrap();
    assert_eq!(value, 0);

    let data = vec![0];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = VarInt::decode(&mut reader).unwrap();
    assert_eq!(value, 0);
}

#[test]
fn test_decode_varint_1() {
    let data = vec![0b10011000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = VarInt::decode(&mut reader).unwrap();
    assert_eq!(value, 1);
}

#[test]
fn test_decode_varint_2() {
    let data = vec![0b10001100];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = VarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 2u64));
    assert_eq!(value, 2);
}

#[test]
fn test_decode_varint_3() {
    let data = vec![0b10001110];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = VarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 3u64));
    assert_eq!(value, 3);
}
