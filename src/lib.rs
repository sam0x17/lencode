#![cfg_attr(not(feature = "std"), no_std)]

pub mod io;

pub mod prelude {
    pub use crate::io::*;
}

use prelude::*;

pub trait Encode {
    fn encode<E>(&self, writer: impl Write) -> Result<usize, E>;
}
