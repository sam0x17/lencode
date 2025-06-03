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

pub mod prelude {
    pub use crate::io::*;
}

use prelude::*;

pub type Result<T> = core::result::Result<T, Error>;

pub trait Encode {
    fn to_bytes(&self, writer: impl Write) -> Result<usize>;
}

pub trait Decode {
    fn from_bytes(reader: impl Read, len: usize) -> Result<Self>
    where
        Self: Sized;
}

pub fn decode<T: Decode>(_reader: &mut BitReader<impl Read>) -> Result<T> {
    todo!()
}
