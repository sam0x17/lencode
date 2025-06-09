use core::fmt::{Debug, Display};
use core::ops::*;

use endian_cast::Endianness;

use crate::prelude::*;

pub mod lencode;

pub trait Scheme {
    fn encode<I: UnsignedInteger>(val: I, writer: impl Write) -> Result<usize>;
    fn decode<I: UnsignedInteger>(reader: impl Read) -> Result<I>;
}

pub trait One {
    const ONE: Self;
}

pub trait Zero {
    const ZERO: Self;
}

pub trait Max {
    const MAX_VALUE: Self;
}

pub trait Min {
    const MIN_VALUE: Self;
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
    + One
    + Zero
    + Max
    + Min
{
    fn encode_uint<S: Scheme>(self, writer: impl Write) -> Result<usize> {
        S::encode(self, writer)
    }
    fn decode_uint<S: Scheme>(reader: impl Read) -> Result<Self> {
        S::decode(reader)
    }
}

#[macro_export]
macro_rules! impl_unsigned_integer {
    ($($t:ty),*) => {
        $(
            impl $crate::varint::One for $t {
                const ONE: Self = 1;
            }
            impl $crate::varint::Zero for $t {
                const ZERO: Self = 0;
            }
            impl $crate::varint::Max for $t {
                const MAX_VALUE: Self = <$t>::MAX;
            }
            impl $crate::varint::Min for $t {
                const MIN_VALUE: Self = <$t>::MIN;
            }
            impl $crate::varint::UnsignedInteger for $t {}
        )*
    };
}

impl_unsigned_integer!(u8, u16, u32, u64, u128, usize);
