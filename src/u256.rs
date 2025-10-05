//! A compact [`U256`] newtype with varint and endianness support.
//!
//! This module exposes [`U256`], a 256‑bit unsigned integer backed by `ruint` and integrated
//! with this crate’s integer helper traits, enabling varint encoding via [`Lencode`].
use crate::prelude::*;

use core::ops::{Shl, ShlAssign, Shr, ShrAssign};
use endian_cast::Endianness;
use generic_array::GenericArray;
use ruint::aliases::U256 as U256Base;
use ruint::uint;

use newt_hype::*;

newtype!(U256, CustomPrimitiveBase, U256Base);

impl One for U256 {
    const ONE: Self = U256::new(uint!(1U256));
}
impl Zero for U256 {
    const ZERO: Self = U256::new(uint!(0U256));
}
impl OneHundredTwentySeven for U256 {
    const ONE_HUNDRED_TWENTY_SEVEN: Self = U256::new(uint!(127U256));
}

impl Max for U256 {
    const MAX_VALUE: Self = U256::new(U256Base::MAX);
}

impl Min for U256 {
    const MIN_VALUE: Self = U256::new(U256Base::MIN);
}
impl ByteLength for U256 {
    const BYTE_LENGTH: usize = core::mem::size_of::<U256>();
}

impl Endianness for U256 {
    type N = generic_array::typenum::U32;

    #[inline(always)]
    fn le_bytes(&self) -> GenericArray<u8, Self::N> {
        const BYTES: usize = 32;
        GenericArray::from(self.0.to_le_bytes::<BYTES>())
    }

    #[inline(always)]
    fn be_bytes(&self) -> GenericArray<u8, Self::N> {
        const BYTES: usize = 32;
        GenericArray::from(self.0.to_be_bytes::<BYTES>())
    }
}

impl Shl<u8> for U256 {
    type Output = Self;

    #[inline(always)]
    fn shl(self, rhs: u8) -> Self::Output {
        Self::new(self.0 << rhs)
    }
}

impl ShlAssign<u8> for U256 {
    #[inline(always)]
    fn shl_assign(&mut self, rhs: u8) {
        self.0 <<= rhs;
    }
}

impl Shr<u8> for U256 {
    type Output = Self;

    #[inline(always)]
    fn shr(self, rhs: u8) -> Self::Output {
        Self::new(self.0 >> rhs)
    }
}

impl ShrAssign<u8> for U256 {
    #[inline(always)]
    fn shr_assign(&mut self, rhs: u8) {
        self.0 >>= rhs;
    }
}

impl UnsignedInteger for U256 {}

impl From<u8> for U256 {
    #[inline(always)]
    fn from(value: u8) -> Self {
        Self::new(U256Base::from(value))
    }
}

impl From<u16> for U256 {
    #[inline(always)]
    fn from(value: u16) -> Self {
        Self::new(U256Base::from(value))
    }
}

impl From<u32> for U256 {
    #[inline(always)]
    fn from(value: u32) -> Self {
        Self::new(U256Base::from(value))
    }
}

impl From<u64> for U256 {
    #[inline(always)]
    fn from(value: u64) -> Self {
        Self::new(U256Base::from(value))
    }
}

impl From<u128> for U256 {
    #[inline(always)]
    fn from(value: u128) -> Self {
        Self::new(U256Base::from(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(feature = "std"))]
    use alloc::vec::Vec;

    fn payload_len(bytes: &[u8]) -> usize {
        bytes
            .iter()
            .rposition(|&b| b != 0)
            .map_or(1, |idx| idx + 1)
    }

    #[test]
    fn test_u256() {
        let a = U256::new(uint!(3749384739874982798749827982479879287984798U256));
        let b = U256::new(uint!(38473878979879837598792422429889U256));
        assert_eq!(a + b - b, a);
    }

    #[test]
    fn test_u256_one_constant() {
        // Basic sanity for ONE and ZERO
        assert!(U256::ONE != U256::ZERO);
        assert_eq!(U256::ONE + U256::ONE, U256::from(2u8));
    }

    #[test]
    fn u256_encode_decode_small_values_roundtrip() {
        for raw in 0u8..=127 {
            let value = U256::from(raw);
            let mut buf = Vec::new();
            let written = value.encode(&mut buf).unwrap();
            assert_eq!(written, 1);
            assert_eq!(buf.len(), 1);
            assert_eq!(buf[0], raw);

            let mut cursor = Cursor::new(buf.as_slice());
            let decoded = U256::decode(&mut cursor).unwrap();
            assert_eq!(decoded, value);
        }
    }

    #[test]
    fn u256_encode_decode_large_values_roundtrip() {
        let cases = [
            U256::from(128u16),
            U256::from(255u16),
            U256::from(256u32),
            U256::from(65535u32),
            U256::from(65536u32),
            U256::from(1u64) << 63,
            U256::from(1u64) << 64,
            (U256::from(1u128) << 120) + U256::from(0x1122_3344_5566_7788u64),
            (U256::from(1u128) << 200)
                + (U256::from(1u64) << 100)
                + U256::from(0xA5A5u16),
        ];

        for value in cases {
            let mut buf = Vec::new();
            let written = value.encode(&mut buf).unwrap();
            assert_eq!(written, buf.len());
            assert!(buf.len() > 1);

            let le = value.le_bytes();
            let payload = payload_len(&le);
            assert!(payload <= 0x7F);
            assert_eq!(buf[0], 0x80 | payload as u8);
            assert_eq!(&buf[1..], &le[..payload]);

            let mut cursor = Cursor::new(buf.as_slice());
            let decoded = U256::decode(&mut cursor).unwrap();
            assert_eq!(decoded, value);
        }
    }

    #[test]
    fn u256_encode_decode_max_value_roundtrip() {
        let value = U256::MAX_VALUE;
        let mut buf = Vec::new();
        let written = value.encode(&mut buf).unwrap();
        assert_eq!(written, buf.len());
        assert_eq!(buf.len(), 33);

        let le = value.le_bytes();
        assert_eq!(buf[0], 0x80 | 32);
        assert_eq!(&buf[1..], &le[..]);

        let mut cursor = Cursor::new(buf.as_slice());
        let decoded = U256::decode(&mut cursor).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn u256_decode_errors_on_truncated_payload() {
        let bytes = [0x83];
        let mut cursor = Cursor::new(&bytes[..]);
        let err = U256::decode(&mut cursor).unwrap_err();
        assert!(matches!(err, Error::ReaderOutOfData));
    }
}
