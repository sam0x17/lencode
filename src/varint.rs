use core::fmt::{Debug, Display};

use crate::prelude::*;

pub mod leb128;

pub trait Scheme {
    fn encode<I: Integer>(writer: impl Write) -> Result<usize>;
    fn decode<I: Integer>(reader: impl Read) -> Result<I>;
}

pub trait Integer: Sized + Copy + PartialEq + Eq + Debug + Display {
    fn encode_int<S: Scheme>(writer: impl Write) -> Result<usize>;
    fn decode_int<S: Scheme>(reader: impl Read) -> Result<S>;
}
