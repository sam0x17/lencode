use core::fmt::{Debug, Display};
use core::ops::*;

use crate::prelude::*;

pub mod leb128;

pub trait Scheme {
    fn encode<I: Integer>(val: I, writer: impl Write) -> Result<usize>;
    fn decode<I: Integer>(reader: impl Read) -> Result<I>;
}

pub trait Integer:
    Sized
    + Copy
    + PartialEq
    + Eq
    + Debug
    + Display
    + Default
    + Shl
    + ShlAssign
    + Shr
    + ShrAssign
    + BitAnd
    + BitAndAssign
    + BitOr
    + BitOrAssign
    + Add
    + AddAssign
    + Sub
    + SubAssign
    + Mul
    + MulAssign
    + Div
    + DivAssign
{
    fn encode_int<S: Scheme>(self, writer: impl Write) -> Result<usize> {
        S::encode(self, writer)
    }
    fn decode_int<S: Scheme>(reader: impl Read) -> Result<Self> {
        S::decode(reader)
    }
}
