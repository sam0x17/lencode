use crate::prelude::*;

use ruint::aliases::U256 as U256Base;

use newt_hype::*;

newtype!(U256, CustomPrimitiveBase, U256Base);

#[test]
fn test_u256() {
    use ruint::uint;
    let a = U256::new(uint!(3749384739874982798749827982479879287984798U256));
    let b = U256::new(uint!(38473878979879837598792422429889U256));
    assert_eq!(a + b - b, a);
}
