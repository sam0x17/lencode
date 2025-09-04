use lencode::prelude::Encode;

#[derive(Encode)]
pub struct Foo {
    pub a: u128,
    pub b: bool,
    pub c: [u64; 18],
}

#[derive(Encode)]
pub enum Bar {
    A(u32),
    B { x: String, y: Vec<u8> },
    C,
}
