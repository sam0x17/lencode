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

pub trait Encode<S: Scheme = Lencode> {
    fn encode(&self, writer: impl Write) -> Result<usize>;
}

pub trait Decode<S: Scheme = Lencode> {
    fn decode(reader: impl Read) -> Result<Self>
    where
        Self: Sized;
}

macro_rules! impl_encode_decode_unsigned_primitive {
    ($($t:ty),*) => {
        $(
            impl<S: Scheme> Encode<S> for $t {
                #[inline(always)]
                fn encode(&self, writer: impl Write) -> Result<usize> {
                    S::encode_varint(*self, writer)
                }
            }

            impl<S: Scheme> Decode<S> for $t {
                #[inline(always)]
                fn decode(reader: impl Read) -> Result<Self> {
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
            impl<S: Scheme> Encode<S> for $t {
                #[inline(always)]
                fn encode(&self, writer: impl Write) -> Result<usize> {
                    S::encode_varint_signed(*self, writer)
                }
            }

            impl<S: Scheme> Decode<S> for $t {
                #[inline(always)]
                fn decode(reader: impl Read) -> Result<Self> {
                    S::decode_varint_signed(reader)
                }
            }
        )*
    };
}

impl_encode_decode_signed_primitive!(i16, i32, i64, i128, isize);

#[test]
fn test_encode_decode_i16_all() {
    for i in -32768..=32767 {
        let val: i16 = i;
        let mut buf = [0u8; 2];
        let n = i16::encode(&val, Cursor::new(&mut buf[..])).unwrap();
        let decoded = i16::decode(Cursor::new(&buf[..n])).unwrap();
        assert_eq!(decoded, val);
    }
}
