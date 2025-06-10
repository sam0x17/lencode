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
    fn encode<S: Scheme>(&self, writer: impl Write) -> Result<usize>;
}

pub trait Decode {
    fn decode<S: Scheme>(reader: impl Read) -> Result<Self>
    where
        Self: Sized;
}

macro_rules! impl_encode_decode_unsigned_primitive {
    ($($t:ty),*) => {
        $(
            impl Encode for $t {
                #[inline(always)]
                fn encode<S: Scheme>(&self, writer: impl Write) -> Result<usize> {
                    S::encode_varint(*self, writer)
                }
            }

            impl Decode for $t {
                #[inline(always)]
                fn decode<S: Scheme>(reader: impl Read) -> Result<Self> {
                    S::decode_varint(reader)
                }
            }
        )*
    };
}

impl_encode_decode_unsigned_primitive!(u16, u32, u64, u128, usize);

macro_rules! impl_encode_decode_signed_primitive {
    ($($t:ty),*) => {
        $(
            impl Encode for $t {
                #[inline(always)]
                fn encode<S: Scheme>(&self, writer: impl Write) -> Result<usize> {
                    S::encode_varint_signed(*self, writer)
                }
            }

            impl Decode for $t {
                #[inline(always)]
                fn decode<S: Scheme>(reader: impl Read) -> Result<Self> {
                    S::decode_varint_signed(reader)
                }
            }
        )*
    };
}

impl_encode_decode_signed_primitive!(i16, i32, i64, i128, isize);

#[test]
fn test_encode_decode_i16_all() {
    for i in i16::MIN..=i16::MAX {
        let val: i16 = i;
        let mut buf = [0u8; 3];
        let n = i16::encode::<Lencode>(&val, Cursor::new(&mut buf[..])).unwrap();
        let decoded = i16::decode::<Lencode>(Cursor::new(&buf[..n])).unwrap();
        assert_eq!(decoded, val);
    }
}
