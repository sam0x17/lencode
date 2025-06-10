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

// when using lencode with u8 we bypass the integer encoding scheme so we don't waste bytes
impl Encode<Lencode> for u8 {
    #[inline(always)]
    fn encode(&self, mut writer: impl Write) -> Result<usize> {
        writer.write(&[*self])
    }
}

impl Decode<Lencode> for u8 {
    #[inline(always)]
    fn decode(mut reader: impl Read) -> Result<Self> {
        let mut buf = [0u8; 1];
        reader.read(&mut buf)?;
        Ok(buf[0])
    }
}

#[test]
fn test_encode_decode_lencode_u8_all() {
    for i in 128..=255 {
        let val: u8 = i;
        let mut buf = [0u8; 1];
        let n = u8::encode(&val, Cursor::new(&mut buf[..])).unwrap();
        assert_eq!(n, 1);
        let decoded = u8::decode(Cursor::new(&buf)).unwrap();
        assert_eq!(decoded, val);
    }
}
