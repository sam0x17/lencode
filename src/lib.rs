#![cfg_attr(not(any(feature = "std", test)), no_std)]

pub mod io;
pub mod varint;

pub mod prelude {
    pub use crate::io::*;
}

use prelude::*;

pub trait Encode {
    fn encode<E>(&self, writer: impl Write) -> Result<usize, E>;
}
