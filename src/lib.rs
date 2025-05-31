#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(all(test, not(feature = "std")))]
use alloc::vec;
#[cfg(all(test, not(feature = "std")))]
use alloc::vec::Vec;

pub mod io;

pub mod prelude {
    pub use crate::io::*;
}

use bitvec::vec::BitVec;
use prelude::*;

pub type Result<T> = core::result::Result<T, Error>;

pub trait Encode {
    fn encode(&self, writer: &mut BitWriter<impl Write>) -> Result<usize>;

    fn encode_to_vec(&self) -> Result<BitVec> {
        let mut writer = BitWriter::new(BitVec::new());
        self.encode(&mut writer)?;
        Ok(writer.into_inner()?)
    }
}

pub trait Decode {
    fn decode(reader: &mut BitReader<impl Read>) -> Result<Self>
    where
        Self: Sized;
}
