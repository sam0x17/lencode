use lencode::prelude::Encode;

#[derive(Encode)]
pub struct Foo {
    pub a: u128,
    pub b: bool,
    pub c: [u64; 18],
}
