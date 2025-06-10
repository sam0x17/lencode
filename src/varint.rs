use core::fmt::{Debug, Display};
use core::ops::*;

use endian_cast::Endianness;

use crate::prelude::*;

pub mod lencode;

/// A trait describing a serialization scheme for unsigned integers.
pub trait Scheme {
    /// Encodes an unsigned integer value using the scheme, writing to the given writer.
    fn encode_varint<I: UnsignedInteger>(val: I, writer: impl Write) -> Result<usize>;
    /// Decodes an unsigned integer value using the scheme, reading from the given reader.
    fn decode_varint<I: UnsignedInteger>(reader: impl Read) -> Result<I>;
}

/// Trait for types that have a constant representing the value one.
pub trait One {
    /// The value one for this type.
    const ONE: Self;
}

/// Trait for types that have a constant representing the value zero.
pub trait Zero {
    /// The value zero for this type.
    const ZERO: Self;
}

/// Trait for types that have a constant representing the maximum value.
pub trait Max {
    /// The maximum value for this type.
    const MAX_VALUE: Self;
}

/// Trait for types that have a constant representing the minimum value.
pub trait Min {
    /// The minimum value for this type.
    const MIN_VALUE: Self;
}

/// Trait for types that have a constant representing their byte length.
pub trait ByteLength {
    /// The number of bytes in this type.
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
    + ToSigned
{
    fn encode_uint<S: Scheme>(self, writer: impl Write) -> Result<usize> {
        S::encode_varint(self, writer)
    }
    fn decode_uint<S: Scheme>(reader: impl Read) -> Result<Self> {
        S::decode_varint(reader)
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

/// Trait for converting a signed integer to its unsigned equivalent.
pub trait ToUnsigned {
    /// The corresponding unsigned type for this signed type.
    type Unsigned: UnsignedInteger + ToSigned<Signed = Self>;
    /// Converts this value to its unsigned representation.
    fn to_unsigned(self) -> Self::Unsigned;
}

/// Trait for converting an unsigned integer to its signed equivalent.
pub trait ToSigned {
    /// The corresponding signed type for this unsigned type.
    type Signed: SignedInteger + ToUnsigned<Unsigned = Self>;
    /// Converts this value to its signed representation.
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

/// Encodes a [`SignedInteger`] into its [`UnsignedInteger`] representation using ZigZag encoding.
#[inline(always)]
pub fn zigzag_encode<I: SignedInteger + ToUnsigned>(value: I) -> <I as ToUnsigned>::Unsigned {
    let bits = I::BYTE_LENGTH * 8;
    let shifted = (value << 1) ^ (value >> (bits as u8 - 1));
    shifted.to_unsigned()
}

/// Decodes an [`UnsignedInteger`] back into its [`SignedInteger`] representation using ZigZag encoding.
#[inline(always)]
pub fn zigzag_decode<U: UnsignedInteger + ToSigned>(value: U) -> <U as ToSigned>::Signed {
    let signed = (value >> 1).to_signed();
    let mask = -((value & U::ONE).to_signed());
    signed ^ mask
}

/// Trait for all signed integer types supported by this crate.
///
/// This trait is automatically implemented for all primitive signed integer types.
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
    + ToUnsigned
{
    /// Encodes this signed integer using the given [`Scheme`] and ZigZag encoding.
    fn encode_int<S: Scheme>(self, writer: impl Write) -> Result<usize> {
        zigzag_encode(self).encode_uint::<S>(writer)
    }

    /// Decodes a signed integer using the given [`Scheme`] and ZigZag decoding.
    fn decode_int<S: Scheme>(reader: impl Read) -> Result<Self> {
        Ok(zigzag_decode(
            <Self as ToUnsigned>::Unsigned::decode_uint::<S>(reader)?,
        ))
    }
}

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

#[test]
fn zigzag_roundtrip_i16_all() {
    for i in 0..=i16::MAX {
        let val: i16 = i;
        let encoded = zigzag_encode(val);
        let decoded = zigzag_decode(encoded);
        if decoded != val {
            panic!("FAIL: val={} encoded={} decoded={}", val, encoded, decoded);
        }
        assert_eq!(decoded, val);
    }
    for i in (i16::MIN + 1)..=0 {
        let val: i16 = i;
        let encoded = zigzag_encode(val);
        let decoded = zigzag_decode(encoded);
        if decoded != val {
            panic!("FAIL: val={} encoded={} decoded={}", val, encoded, decoded);
        }
        assert_eq!(decoded, val);
    }
}

#[test]
fn zigzag_roundtrip_i32_all() {
    for i in 0..=i32::MAX {
        let val: i32 = i;
        let encoded = zigzag_encode(val);
        let decoded = zigzag_decode(encoded);
        if decoded != val {
            panic!("FAIL: val={} encoded={} decoded={}", val, encoded, decoded);
        }
        assert_eq!(decoded, val);
    }
    for i in (i32::MIN + 1)..=0 {
        let val: i32 = i;
        let encoded = zigzag_encode(val);
        let decoded = zigzag_decode(encoded);
        if decoded != val {
            panic!("FAIL: val={} encoded={} decoded={}", val, encoded, decoded);
        }
        assert_eq!(decoded, val);
    }
}
