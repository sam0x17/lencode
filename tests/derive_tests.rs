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
    let mut encoder = DedupeEncoder::new();
    let bytes_written = original
        .encode_ext(&mut buffer, Some(&mut encoder))
        .unwrap();
    assert!(bytes_written > 0);

    let mut cursor = Cursor::new(&buffer);
    let mut decoder = DedupeDecoder::new();
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
    let mut encoder = DedupeEncoder::new();
    let bytes_written = original
        .encode_ext(&mut buffer, Some(&mut encoder))
        .unwrap();
    assert!(bytes_written > 0);

    let mut cursor = Cursor::new(&buffer);
    let mut decoder = DedupeDecoder::new();
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
