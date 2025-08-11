use crate::prelude::*;

use core::ops::{Shl, ShlAssign, Shr, ShrAssign};
use endian_cast::Endianness;
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
}

// The generated newtype does not provide implementations for shifting by
// primitive integers.  Our [`UnsignedInteger`] trait requires support for
// shifting by `u8`, so we manually forward these operations to the underlying
// `ruint` type.
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

#[test]
fn test_u256() {
    let a = U256::new(uint!(3749384739874982798749827982479879287984798U256));
    let b = U256::new(uint!(38473878979879837598792422429889U256));
    assert_eq!(a + b - b, a);
}
