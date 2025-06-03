use endian_cast::Endianness;

use crate::io::{BitReader, BitWriter, Read, Write};
use crate::*;
use bitvec::prelude::*;

pub trait BitVarInt: Endianness + Default + Eq + core::fmt::Debug {
    /// Encodes the value into raw bits using the len4 encoding scheme.
    fn encode<W: Write, const N: usize>(self, writer: &mut BitWriter<W, Msb0, N>) -> Result<usize> {
        let mut bits_written = 0;
        if self == Self::default() {
            // if the value is zero, we write a single 0 bit
            writer.write_bit(false)?;
            return Ok(1);
        }
        // if the value is non-zero, we write a 1 bit, then a run of 1s, a run of 0s, and then
        // the value bits
        writer.write_bit(true)?;
        bits_written += 1;
        let bitsize = core::mem::size_of::<Self>() * 8;
        // each 1 adds 4 to the bitsize of the value
        for _ in 0..(bitsize / 4) {
            writer.write_bit(true)?;
            bits_written += 1;
        }
        // sentinel bit for the run of 1s
        writer.write_bit(false)?;
        bits_written += 1;
        // each 0 adds 1 to the bitsize of the value
        for _ in 0..(bitsize % 4) {
            writer.write_bit(false)?;
            bits_written += 1;
        }
        writer.write_bit(true)?; // sentinel bit
        bits_written += 1;
        let bytes = self.le_bytes();
        writer.write(&bytes)?;
        bits_written += bytes.len() * 8;
        Ok(bits_written)
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

    fn to_varint_bits(&self) -> Result<(Vec<u8>, usize)> {
        let mut writer = BitWriter::<_>::new(Vec::<u8>::new());
        let bits_written = self.encode(&mut writer)?;
        Ok((writer.into_inner()?, bits_written))
    }

    fn from_varint_bytes(bytes: &[u8]) -> Result<Self> {
        let mut reader = BitReader::<_>::new(Cursor::new(bytes));
        Self::decode(&mut reader)
    }
}

impl BitVarInt for u8 {}
impl BitVarInt for u16 {}
impl BitVarInt for u32 {}
impl BitVarInt for u64 {}
impl BitVarInt for u128 {}
impl BitVarInt for usize {}

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
    let value: u64 = BitVarInt::decode(&mut reader).unwrap();
    assert_eq!(value, 0);

    let data = vec![0];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = BitVarInt::decode(&mut reader).unwrap();
    assert_eq!(value, 0);
}

#[test]
fn test_decode_varint_1() {
    let data = vec![0b10011000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = BitVarInt::decode(&mut reader).unwrap();
    assert_eq!(value, 1);
}

#[test]
fn test_decode_varint_2() {
    let data = vec![0b10001100];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = BitVarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 2u64));
    assert_eq!(value, 2);
}

#[test]
fn test_decode_varint_3() {
    let data = vec![0b10001110];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = BitVarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 3u64));
    assert_eq!(value, 3);
}

#[test]
fn test_decode_varint_4() {
    let data = vec![0b10000110, 0b00000000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = BitVarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 4u64));
    assert_eq!(value, 4);
}

#[test]
fn test_decode_varint_5() {
    let data = vec![0b10000110, 0b10000000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = BitVarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 5u64));
    assert_eq!(value, 5);
}

#[test]
fn test_decode_varint_6() {
    let data = vec![0b10000111, 0b00000000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = BitVarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 6u64));
    assert_eq!(value, 6);
}

#[test]
fn test_decode_varint_7() {
    let data = vec![0b10000111, 0b10000000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = BitVarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 7u64));
    assert_eq!(value, 7);
}

#[test]
fn test_decode_varint_8() {
    let data = vec![0b11011000, 0b00000000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = BitVarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 8u64));
    assert_eq!(value, 8);
}

#[test]
fn test_decode_varint_9() {
    let data = vec![0b11011001, 0b00000000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = BitVarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 9u64));
    assert_eq!(value, 9);
}

#[test]
fn test_decode_varint_10() {
    let data = vec![0b11011010, 0b00000000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = BitVarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 10u64));
    assert_eq!(value, 10);
}

#[test]
fn test_decode_varint_11() {
    let data = vec![0b11011011, 0b00000000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = BitVarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 11u64));
    assert_eq!(value, 11);
}

#[test]
fn test_decode_varint_12() {
    let data = vec![0b11011100, 0b00000000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = BitVarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 12u64));
    assert_eq!(value, 12);
}

#[test]
fn test_decode_varint_114() {
    let data = vec![0b1100_0011, 0b1100_1000];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = BitVarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 114u64));
    assert_eq!(value, 114);
}

#[test]
fn test_decode_varint_507() {
    let data = vec![0b1110_0111, 0b1111_0110];
    let mut reader = BitReader::<_>::new(Cursor::new(data));
    let value: u64 = BitVarInt::decode(&mut reader).unwrap();
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
    let value: u64 = BitVarInt::decode(&mut reader).unwrap();
    assert_eq!(format!("{:064b}", value), format!("{:064b}", 14387324u64));
    assert_eq!(value, 14387324);
}

#[cfg(feature = "std")]
#[test]
fn test_round_trip_u32_100k() {
    use rayon::prelude::*;
    (0..=100_000).par_bridge().for_each(|i| {
        let value: u32 = i;
        let (bytes, bits_written) = value.to_varint_bits().unwrap();
        let mut writer = BitWriter::new(bytes);
        assert_eq!(bits_written, value.encode::<_, 64>(&mut writer).unwrap());
        let bytes = writer.into_inner().unwrap();
        let decoded_value = u32::from_varint_bytes(&bytes).unwrap();
        assert_eq!(decoded_value, value);
    });
}

#[cfg(feature = "std")]
#[test]
fn test_round_trip_u64_100k() {
    use rayon::prelude::*;
    (0..=100_000).par_bridge().for_each(|i| {
        let value: u64 = i;
        let (bytes, bits_written) = value.to_varint_bits().unwrap();
        let mut writer = BitWriter::new(bytes);
        assert_eq!(bits_written, value.encode::<_, 64>(&mut writer).unwrap());
        let bytes = writer.into_inner().unwrap();
        let decoded_value = u64::from_varint_bytes(&bytes).unwrap();
        assert_eq!(decoded_value, value);
    });
}

#[cfg(feature = "std")]
#[test]
fn test_round_trip_u128_100k() {
    use rayon::prelude::*;
    (0..=100_000).par_bridge().for_each(|i| {
        let value: u128 = i;
        let (bytes, bits_written) = value.to_varint_bits().unwrap();
        let mut writer = BitWriter::new(bytes);
        assert_eq!(bits_written, value.encode::<_, 128>(&mut writer).unwrap());
        let bytes = writer.into_inner().unwrap();
        let decoded_value = u128::from_varint_bytes(&bytes).unwrap();
        assert_eq!(decoded_value, value);
    });
}

#[cfg(feature = "std")]
#[test]
fn test_round_trip_u16_all() {
    use rayon::prelude::*;
    (0..=u16::MAX).par_bridge().for_each(|i| {
        let value: u16 = i;
        let (bytes, bits_written) = value.to_varint_bits().unwrap();
        let mut writer = BitWriter::new(bytes);
        assert_eq!(bits_written, value.encode::<_, 64>(&mut writer).unwrap());
        let bytes = writer.into_inner().unwrap();
        let decoded_value = u16::from_varint_bytes(&bytes).unwrap();
        assert_eq!(decoded_value, value);
    });
}

#[cfg(feature = "std")]
#[test]
fn test_round_trip_u8_all() {
    use rayon::prelude::*;
    (0..=u8::MAX).par_bridge().for_each(|i| {
        let value: u8 = i;
        let (bytes, bits_written) = value.to_varint_bits().unwrap();
        let mut writer = BitWriter::new(bytes);
        assert_eq!(bits_written, value.encode::<_, 64>(&mut writer).unwrap());
        let bytes = writer.into_inner().unwrap();
        let decoded_value = u8::from_varint_bytes(&bytes).unwrap();
        assert_eq!(decoded_value, value);
    });
}

#[cfg(feature = "std")]
#[test]
fn test_round_trip_u32_all() {
    use rayon::prelude::*;
    let target = u32::MAX / 1000;
    (0..=target).par_bridge().for_each(|i| {
        let value: u32 = i;
        let mut buf = [0u8; 8];
        let mut writer = BitWriter::<_, Msb0, 8>::new(&mut buf[..]); // TODO: make this 2 and it fails!!
        value.encode(&mut writer).unwrap();
        drop(writer);
        let mut reader = BitReader::<_, Msb0, 2>::new(Cursor::new(&buf[..]));
        let decoded_value = u32::decode(&mut reader).unwrap();
        assert_eq!(decoded_value, value);
        // if i % 1000000 == 0 {
        //     println!("{:.2}%", decoded_value as f64 / target as f64 * 100.0);
        // }
    });
}

#[cfg(feature = "std")]
#[test]
fn test_round_trip_u32_all_small_buffer() {
    use rayon::prelude::*;
    let target = u32::MAX / 1000;
    //(0..=target).par_bridge().for_each(|i| {
    (0..=target).for_each(|i| {
        let value: u32 = i;
        let mut buf = [0u8; 8];
        let mut writer = BitWriter::<_, Msb0, 1>::new(&mut buf[..]);
        value.encode(&mut writer).unwrap();
        drop(writer);
        let mut reader = BitReader::<_, Msb0, 1>::new(Cursor::new(&buf[..]));
        let decoded_value = u32::decode(&mut reader).unwrap();
        assert_eq!(decoded_value, value);
        // if i % 1000000 == 0 {
        //     println!("{:.2}%", decoded_value as f64 / target as f64 * 100.0);
        // }
    });
}
