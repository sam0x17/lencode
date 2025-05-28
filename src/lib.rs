#![cfg_attr(not(feature = "std"), no_std)]

pub mod io;

pub mod prelude {
    pub use crate::io::*;
}

use prelude::*;

pub type Result<T> = core::result::Result<T, Error>;

pub trait Encode {
    fn encode(&self, writer: impl Write) -> Result<usize>;
}
