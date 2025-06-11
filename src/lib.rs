#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(all(test, not(feature = "std")))]
use alloc::format;
#[cfg(all(test, not(feature = "std")))]
use alloc::vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

pub mod bit_varint;
pub mod io;
pub mod varint;

pub mod prelude {
    pub use super::*;
    pub use crate::bit_varint::*;
    pub use crate::io::*;
    pub use crate::varint::lencode::*;
    pub use crate::varint::*;
}

use prelude::*;

pub type Result<T> = core::result::Result<T, Error>;

pub trait Encode {
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize>;

    fn encode_len<S: Scheme>(len: usize, writer: &mut impl Write) -> Result<usize> {
        S::encode_varint(len as u64, writer)
    }
}

pub trait Decode {
    fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self>
    where
        Self: Sized;

    fn decode_len<S: Scheme>(reader: &mut impl Read) -> Result<usize> {
        S::decode_varint::<u64>(reader).map(|v| v as usize)
    }
}

macro_rules! impl_encode_decode_unsigned_primitive {
    ($($t:ty),*) => {
        $(
            impl Encode for $t {
                #[inline(always)]
                fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
                    S::encode_varint(*self, writer)
                }
            }

            impl Decode for $t {
                #[inline(always)]
                fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
                    S::decode_varint(reader)
                }

                #[inline(always)]
                fn decode_len<S: Scheme>(_reader: &mut impl Read) -> Result<usize> {
                    unimplemented!()
                }
            }
        )*
    };
}

impl_encode_decode_unsigned_primitive!(u16, u32, u64, u128);

impl Encode for usize {
    #[inline(always)]
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
        S::encode_varint(*self as u64, writer)
    }
}

impl Decode for usize {
    #[inline(always)]
    fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
        S::decode_varint(reader).map(|v: u64| v as usize)
    }

    #[inline(always)]
    fn decode_len<S: Scheme>(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

macro_rules! impl_encode_decode_signed_primitive {
    ($($t:ty),*) => {
        $(
            impl Encode for $t {
                #[inline(always)]
                fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
                    S::encode_varint_signed(*self, writer)
                }
            }

            impl Decode for $t {
                #[inline(always)]
                fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
                    S::decode_varint_signed(reader)
                }

                #[inline(always)]
                fn decode_len<S: Scheme>(_reader: &mut impl Read) -> Result<usize> {
                    unimplemented!()
                }
            }
        )*
    };
}

impl_encode_decode_signed_primitive!(i16, i32, i64, i128);

impl Encode for isize {
    #[inline(always)]
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
        S::encode_varint_signed(*self as i64, writer)
    }
}

impl Decode for isize {
    #[inline(always)]
    fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
        S::decode_varint_signed(reader).map(|v: i64| v as isize)
    }

    #[inline(always)]
    fn decode_len<S: Scheme>(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<T: Decode> Decode for Vec<T> {
    fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
        let len = Self::decode_len::<S>(reader)?;
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(T::decode::<S>(reader)?);
        }
        Ok(vec)
    }
}

impl<T: Encode> Encode for Vec<T> {
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len::<S>(self.len(), writer)?;
        for item in self {
            total_written += item.encode::<S>(writer)?;
        }
        Ok(total_written)
    }
}

#[test]
fn test_encode_decode_i16_all() {
    for i in i16::MIN..=i16::MAX {
        let val: i16 = i;
        let mut buf = [0u8; 3];
        let n = i16::encode::<Lencode>(&val, &mut Cursor::new(&mut buf[..])).unwrap();
        let decoded = i16::decode::<Lencode>(&mut Cursor::new(&buf[..n])).unwrap();
        assert_eq!(decoded, val);
    }
}

#[test]
fn test_encode_decode_vec_of_i16_all() {
    let values: Vec<i16> = (i16::MIN..=i16::MAX).collect();
    let mut buf = vec![0u8; 3 * values.len()];
    let n = values
        .encode::<Lencode>(&mut Cursor::new(&mut buf[..]))
        .unwrap();
    assert!(n < values.len() * 3);
    let decoded = Vec::<i16>::decode::<Lencode>(&mut Cursor::new(&buf[..n])).unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn test_encode_decode_vec_of_many_small_u128() {
    let values: Vec<u128> = (0..(u16::MAX / 2) as u128)
        .chain(0..(u16::MAX / 2) as u128)
        .collect();
    let mut buf = vec![0u8; 3 * values.len()];
    let n = values
        .encode::<Lencode>(&mut Cursor::new(&mut buf[..]))
        .unwrap();
    assert!(n < values.len() * 3);
    let decoded = Vec::<u128>::decode::<Lencode>(&mut Cursor::new(&buf[..n])).unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn test_encode_decode_vec_of_tiny_u128s() {
    let values: Vec<u128> = (0..127).collect();
    let mut buf = vec![0u8; values.len() + 1];
    let n = values
        .encode::<Lencode>(&mut Cursor::new(&mut buf[..]))
        .unwrap();
    assert_eq!(n, values.len() + 1);
    let decoded = Vec::<u128>::decode::<Lencode>(&mut Cursor::new(&buf[..n])).unwrap();
    assert_eq!(decoded, values);
}
