use endian_cast::Endianness;

use crate::io::{BitReader, BitWriter, Read, Write};
use crate::*;
use bitvec::prelude::*;

pub trait VarInt: Endianness + Default + Eq + core::fmt::Debug {
    /// Encodes the value into raw bits using the len4 encoding scheme.
    fn encode<W: Write, const N: usize>(self, writer: &mut BitWriter<W, N>) -> Result<usize> {
        if self == Self::default() {
            // if the value is zero, we write a single 0 bit
            writer.write_bit(false)?;
            return Ok(1);
        }
        // if the value is non-zero, we write a 1 bit, then a run of 1s, a run of 0s, and then
        // the value bits
        writer.write_bit(true)?;
        let bitsize = core::mem::size_of::<Self>() * 8;
        // each 1 adds 4 to the bitsize of the value
        for _ in 0..(bitsize / 4) {
            writer.write_bit(true)?;
        }
        // sentinel bit for the run of 1s
        writer.write_bit(false)?;
        // each 0 adds 1 to the bitsize of the value
        for _ in 0..(bitsize % 4) {
            writer.write_bit(false)?;
        }
        writer.write_bit(true)?; // sentinel bit
        let bytes = self.le_bytes();
        writer.write(&bytes)
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
            // first bit 1 means the value is non-zero and we need to read run of 1s, run of
            // 0s, and then the value bits
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
        } else {
            // first bit 0 means the value is 0 and we are done
            return Ok(val);
        }
        // reverse byte order if we are big-endian
        #[cfg(target_endian = "big")]
        reverse(buf);
        Ok(val)
    }

    // fn to_varint_bytes(&self) -> Result<Vec<u8>> {
    //     let mut writer = BitWriter::<_, 32, Msb0>::new(Vec::<u8>::new());
    //     self.encode(&mut writer)?;
    //     Ok(writer.into_inner()?)
    // }

    // fn from_varint_bytes(bytes: &[u8]) -> Result<Self> {
    //     let mut reader = BitReader::<&[u8], Msb0, 64>::new(bytes);
    //     Self::decode(&mut reader)
    // }
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

#[test]
fn test_decode_varint_4() {
    let data = vec![0b10000110, 0b00000000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = VarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 4u64));
    assert_eq!(value, 4);
}

#[test]
fn test_decode_varint_5() {
    let data = vec![0b10000110, 0b10000000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = VarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 5u64));
    assert_eq!(value, 5);
}

#[test]
fn test_decode_varint_6() {
    let data = vec![0b10000111, 0b00000000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = VarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 6u64));
    assert_eq!(value, 6);
}

#[test]
fn test_decode_varint_7() {
    let data = vec![0b10000111, 0b10000000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = VarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 7u64));
    assert_eq!(value, 7);
}

#[test]
fn test_decode_varint_8() {
    let data = vec![0b11011000, 0b00000000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = VarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 8u64));
    assert_eq!(value, 8);
}

#[test]
fn test_decode_varint_9() {
    let data = vec![0b11011001, 0b00000000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = VarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 9u64));
    assert_eq!(value, 9);
}

#[test]
fn test_decode_varint_10() {
    let data = vec![0b11011010, 0b00000000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = VarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 10u64));
    assert_eq!(value, 10);
}

#[test]
fn test_decode_varint_11() {
    let data = vec![0b11011011, 0b00000000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = VarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 11u64));
    assert_eq!(value, 11);
}

#[test]
fn test_decode_varint_12() {
    let data = vec![0b11011100, 0b00000000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = VarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 12u64));
    assert_eq!(value, 12);
}

#[test]
fn test_decode_varint_114() {
    let data = vec![0b1100_0011, 0b1100_1000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = VarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 114u64));
    assert_eq!(value, 114);
}

#[test]
fn test_decode_varint_507() {
    let data = vec![0b1110_0111, 0b1111_0110];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = VarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 507u64));
    assert_eq!(value, 507);
}

#[test]
fn test_decode_varint_14387324() {
    let data = vec![
        0b1111_1110,
        0b1011_1110,
        0b0100_0100,
        0b0110_1101,
        0b1000_0000,
        0b0000_0000,
    ];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = VarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 14387324u64));
    assert_eq!(value, 14387324);
}
