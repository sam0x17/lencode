use core::fmt::{Debug, Display};
use core::ops::*;

use endian_cast::Endianness;

use crate::prelude::*;

pub mod lencode;

pub trait Scheme {
    fn encode<I: UnsignedInteger>(val: I, writer: impl Write) -> Result<usize>;
    fn decode<I: UnsignedInteger>(reader: impl Read) -> Result<I>;
}

pub trait UnsignedInteger:
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
    + Endianness
{
    fn encode_uint<S: Scheme>(self, writer: impl Write) -> Result<usize> {
        S::encode(self, writer)
    }
    fn decode_uint<S: Scheme>(reader: impl Read) -> Result<Self> {
        S::decode(reader)
    }
}

impl UnsignedInteger for u8 {}
impl UnsignedInteger for u16 {}
impl UnsignedInteger for u32 {}
impl UnsignedInteger for u64 {}
impl UnsignedInteger for u128 {}
impl UnsignedInteger for usize {}
