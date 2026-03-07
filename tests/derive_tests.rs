use lencode::prelude::*;

#[derive(Encode, Decode, Debug, PartialEq)]
pub struct Foo {
    pub a: u128,
    pub b: bool,
    pub c: [u64; 18],
}

#[derive(Encode, Decode, Debug, PartialEq)]
pub enum Bar {
    A(u32),
    B { x: String, y: Vec<u8> },
    C,
}

#[test]
fn test_struct_encode_decode_roundtrip() {
    let original = Foo {
        a: 12345,
        b: true,
        c: [
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18,
        ],
    };

    let mut buffer = Vec::new();
    let bytes_written = original.encode(&mut buffer).unwrap();
    assert!(bytes_written > 0);

    let mut cursor = Cursor::new(&buffer);
    let decoded: Foo = Foo::decode(&mut cursor).unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_enum_encode_decode_roundtrip() {
    let test_cases = vec![
        Bar::A(42),
        Bar::B {
            x: "test".to_string(),
            y: vec![1, 2, 3, 4, 5],
        },
        Bar::C,
    ];

    for original in test_cases {
        let mut buffer = Vec::new();
        let bytes_written = original.encode(&mut buffer).unwrap();
        assert!(bytes_written > 0);

        let mut cursor = Cursor::new(&buffer);
        let decoded: Bar = Bar::decode(&mut cursor).unwrap();

        assert_eq!(original, decoded);
    }
}

#[test]
fn test_struct_with_deduplication() {
    let original = Foo {
        a: 9876543210,
        b: false,
        c: [100; 18], // All the same value for testing
    };

    let mut buffer = Vec::new();
    let mut encoder = EncoderContext::with_dedupe();
    let bytes_written = original
        .encode_ext(&mut buffer, Some(&mut encoder))
        .unwrap();
    assert!(bytes_written > 0);

    let mut cursor = Cursor::new(&buffer);
    let mut decoder = DecoderContext::with_dedupe();
    let decoded: Foo = Foo::decode_ext(&mut cursor, Some(&mut decoder)).unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_enum_with_deduplication() {
    let original = Bar::B {
        x: "hello world".to_string(),
        y: vec![255; 10],
    };

    let mut buffer = Vec::new();
    let mut encoder = EncoderContext::with_dedupe();
    let bytes_written = original
        .encode_ext(&mut buffer, Some(&mut encoder))
        .unwrap();
    assert!(bytes_written > 0);

    let mut cursor = Cursor::new(&buffer);
    let mut decoder = DecoderContext::with_dedupe();
    let decoded: Bar = Bar::decode_ext(&mut cursor, Some(&mut decoder)).unwrap();

    assert_eq!(original, decoded);
}

// regression test
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
#[repr(u8)]
pub enum SiblingPosition {
    Left,
    Right,
}

// derive(Pack) tests

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Pack)]
struct SimplePoint {
    x: u32,
    y: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Pack)]
#[repr(transparent)]
struct MyKey([u8; 32]);

impl DedupeEncodeable for MyKey {}
impl DedupeDecodeable for MyKey {}

#[test]
fn test_derive_pack_named_struct_roundtrip() {
    let p = SimplePoint { x: 42, y: 99 };
    let mut buf = Vec::new();
    p.pack(&mut buf).unwrap();
    let decoded = SimplePoint::unpack(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(p, decoded);
}

#[test]
fn test_derive_pack_transparent_roundtrip() {
    let key = MyKey([0xAB; 32]);
    let mut buf = Vec::new();
    key.pack(&mut buf).unwrap();
    assert_eq!(buf.len(), 32);
    let decoded = MyKey::unpack(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(key, decoded);
}

#[test]
fn test_derive_pack_transparent_bulk_vec_roundtrip() {
    // Test that the derived pack_slice/unpack_vec work via Vec<MyKey> encode/decode
    let keys: Vec<MyKey> = (0..100u8).map(|i| MyKey([i; 32])).collect();
    let mut buf = VecWriter::new();
    encode(&keys, &mut buf).unwrap();
    let decoded: Vec<MyKey> = decode(&mut Cursor::new(buf.as_slice())).unwrap();
    assert_eq!(keys, decoded);
}

#[test]
fn test_derive_pack_transparent_dedupe_roundtrip() {
    // Test deduplication works with the derived Pack
    let keys = vec![
        MyKey([1; 32]),
        MyKey([2; 32]),
        MyKey([1; 32]),
        MyKey([2; 32]),
        MyKey([1; 32]),
    ];

    let mut enc = EncoderContext::with_dedupe();
    let mut buf = VecWriter::new();
    encode_ext(&keys, &mut buf, Some(&mut enc)).unwrap();

    let mut dec = DecoderContext::with_dedupe();
    let decoded: Vec<MyKey> = decode_ext(&mut Cursor::new(buf.as_slice()), Some(&mut dec)).unwrap();
    assert_eq!(keys, decoded);
}
