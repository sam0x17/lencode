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

pub trait ByteLength {
    const BYTE_LENGTH: usize;
}

pub trait UnsignedInteger:
    Sized
    + Copy
    + PartialEq
    + Eq
    + Debug
    + Display
    + Default
    + Shl<u8, Output = Self>
    + ShlAssign
    + Shr<u8, Output = Self>
    + ShrAssign
    + BitAnd<Output = Self>
    + BitAndAssign
    + BitOr<Output = Self>
    + BitOrAssign
    + Add<Output = Self>
    + AddAssign
    + Sub<Output = Self>
    + SubAssign
    + Mul<Output = Self>
    + MulAssign
    + Div<Output = Self>
    + DivAssign
    + Endianness
    + One
    + Zero
    + Max
    + Min
    + ByteLength
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
            impl $crate::varint::ByteLength for $t {
                const BYTE_LENGTH: usize = core::mem::size_of::<$t>();
            }
            impl $crate::varint::UnsignedInteger for $t {}
        )*
    };
}

impl_unsigned_integer!(u8, u16, u32, u64, u128, usize);

// Trait to map signed types to their unsigned equivalents
pub trait ToUnsigned {
    type Unsigned: UnsignedInteger + ToSigned<Signed = Self>;
    fn to_unsigned(self) -> Self::Unsigned;
}

pub trait ToSigned {
    type Signed: SignedInteger + ToUnsigned<Unsigned = Self>;
    fn to_signed(self) -> Self::Signed;
}

macro_rules! impl_to_unsigned_signed {
    ($(($signed:ty, $unsigned:ty)),*) => {
        $(
            impl ToUnsigned for $signed {
                type Unsigned = $unsigned;
                fn to_unsigned(self) -> $unsigned { self as $unsigned }
            }
            impl ToSigned for $unsigned {
                type Signed = $signed;
                fn to_signed(self) -> $signed { self as $signed }
            }
        )*
    };
}

impl_to_unsigned_signed!(
    (i8, u8),
    (i16, u16),
    (i32, u32),
    (i64, u64),
    (i128, u128),
    (isize, usize)
);

// ZigZag encode: signed -> unsigned
pub fn zigzag_encode<I: SignedInteger + ToUnsigned>(value: I) -> <I as ToUnsigned>::Unsigned {
    let bits = I::BYTE_LENGTH * 8;
    let shifted = (value << 1) ^ (value >> (bits as u8 - 1));
    shifted.to_unsigned()
}

// ZigZag decode: unsigned -> signed
pub fn zigzag_decode<U: UnsignedInteger + ToSigned>(value: U) -> <U as ToSigned>::Signed {
    let signed = (value >> 1).to_signed();
    let mask = -((value & U::ONE).to_signed());
    signed ^ mask
}

pub trait SignedInteger:
    Sized
    + Copy
    + PartialEq
    + Eq
    + Debug
    + Display
    + Default
    + Shl<u8, Output = Self>
    + ShlAssign
    + Shr<u8, Output = Self>
    + ShrAssign
    + BitAnd<Output = Self>
    + BitAndAssign
    + BitOr<Output = Self>
    + BitOrAssign
    + Add<Output = Self>
    + AddAssign
    + Sub<Output = Self>
    + SubAssign
    + Mul<Output = Self>
    + MulAssign
    + Div<Output = Self>
    + DivAssign
    + Neg<Output = Self>
    + BitXor<Output = Self>
    + BitXorAssign
    + Endianness
    + One
    + Zero
    + Max
    + Min
    + ByteLength
{
    fn encode_int<S: Scheme>(self, writer: impl Write) -> Result<usize> {
        todo!()
        // S::encode(self, writer)
    }
    fn decode_int<S: Scheme>(reader: impl Read) -> Result<Self> {
        todo!()
        // S::decode(reader)
    }
}

#[macro_export]
macro_rules! impl_signed_integer {
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
            impl $crate::varint::ByteLength for $t {
                const BYTE_LENGTH: usize = core::mem::size_of::<$t>();
            }
            impl $crate::varint::SignedInteger for $t {}
        )*
    };
}

impl_signed_integer!(i8, i16, i32, i64, i128, isize);

#[test]
fn zigzag_encode_decode_i32_roundtrip() {
    let values = [0i32, -1, 1, -2, 2, i32::MAX, i32::MIN + 1];
    for &v in &values {
        let encoded = zigzag_encode(v);
        let decoded = zigzag_decode(encoded);
        assert_eq!(decoded, v, "zigzag roundtrip failed for {}", v);
    }
}

#[test]
fn zigzag_encode_decode_i64_roundtrip() {
    let values = [0i64, -1, 1, -2, 2, i64::MAX, i64::MIN + 1];
    for &v in &values {
        let encoded = zigzag_encode(v);
        let decoded = zigzag_decode(encoded);
        assert_eq!(decoded, v, "zigzag roundtrip failed for {}", v);
    }
}

#[test]
fn zigzag_known_values() {
    // (input, expected zigzag encoding)
    let cases = [
        (0i32, 0u32),
        (-1, 1),
        (1, 2),
        (-2, 3),
        (2, 4),
        (-3, 5),
        (3, 6),
    ];
    for &(input, expected) in &cases {
        let encoded = zigzag_encode(input);
        assert_eq!(encoded, expected, "zigzag_encode({})", input);
    }
}
