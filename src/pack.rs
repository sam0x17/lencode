use crate::prelude::*;

/// Implemented on types that can be packed into a platform-independent byte-stream.
pub trait Pack: Sized {
    fn pack(&self, writer: &mut impl Write) -> Result<usize>;
    fn unpack(reader: &mut impl Read) -> Result<Self>;
}

impl<const N: usize, T: Pack> Pack for [T; N] {
    #[inline]
    fn pack(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_bytes = 0;
        for item in self.iter() {
            total_bytes += item.pack(writer)?;
        }
        Ok(total_bytes)
    }

    #[inline]
    fn unpack(reader: &mut impl Read) -> Result<Self> {
        let mut arr: core::mem::MaybeUninit<[T; N]> = core::mem::MaybeUninit::uninit();
        let ptr = arr.as_mut_ptr() as *mut T;
        for i in 0..N {
            unsafe {
                ptr.add(i).write(T::unpack(reader)?);
            }
        }
        Ok(unsafe { arr.assume_init() })
    }
}

/// Macro to implement the Pack trait for types that implement Endianness.
/// This avoids orphan rule issues by allowing explicit implementations per type.
///
/// # Usage
///
/// ```ignore
/// use lencode::impl_pack_for_endianness_types;
///
/// // For a single type
/// impl_pack_for_endianness_types!(MyType);
///
/// // For multiple types
/// impl_pack_for_endianness_types!(Type1, Type2, Type3);
/// ```
///
/// The macro will generate Pack implementations that:
/// - Use little-endian byte ordering for packing
/// - Validate that the full expected number of bytes are read during unpacking
/// - Return appropriate errors for insufficient data or space
#[macro_export]
macro_rules! impl_pack_for_endianness_types {
    ($($t:ty),+ $(,)?) => {
        $(
            impl $crate::pack::Pack for $t {
                #[inline(always)]
                fn pack(&self, writer: &mut impl $crate::io::Write) -> $crate::Result<usize> {
                    writer.write(&endian_cast::Endianness::le_bytes(self))
                }

                fn unpack(reader: &mut impl $crate::io::Read) -> $crate::Result<Self> {
                    let mut ret = core::mem::MaybeUninit::<Self>::uninit();
                    let buf_slice = unsafe {
                        core::slice::from_raw_parts_mut(
                            ret.as_mut_ptr() as *mut u8,
                            core::mem::size_of::<Self>(),
                        )
                    };
                    let bytes_read = reader.read(buf_slice)?;
                    if bytes_read != core::mem::size_of::<Self>() {
                        return Err($crate::io::Error::ReaderOutOfData);
                    }
                    Ok(unsafe { ret.assume_init() })
                }
            }
        )+
    };
}

// Implement Pack for all the standard primitive types that implement Endianness
impl_pack_for_endianness_types!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize, f32, f64
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::Cursor;

    #[test]
    fn test_macro_usage() {
        // Test that the macro was used correctly for built-in types
        let value: u32 = 0x12345678;
        let mut buffer = vec![0u8; 10];
        let mut cursor = Cursor::new(&mut buffer[..]);

        // This should work because we used the macro to implement Pack for u32
        let bytes_written = value.pack(&mut cursor).unwrap();
        assert_eq!(bytes_written, 4);

        let mut read_cursor = Cursor::new(&buffer[..]);
        let unpacked: u32 = u32::unpack(&mut read_cursor).unwrap();
        assert_eq!(unpacked, value);
    }
}

#[test]
fn test_pack_unpack_u8() {
    let original: u8 = 42;
    let mut buffer = vec![0u8; 10];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 1);
    assert_eq!(buffer[0], 42);

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: u8 = u8::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_pack_unpack_u16() {
    let original: u16 = 0x1234;
    let mut buffer = vec![0u8; 10];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 2);
    // Check little-endian byte order
    assert_eq!(buffer[0], 0x34);
    assert_eq!(buffer[1], 0x12);

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: u16 = u16::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_pack_unpack_u32() {
    let original: u32 = 0x12345678;
    let mut buffer = vec![0u8; 10];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 4);
    // Check little-endian byte order
    assert_eq!(buffer[0], 0x78);
    assert_eq!(buffer[1], 0x56);
    assert_eq!(buffer[2], 0x34);
    assert_eq!(buffer[3], 0x12);

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: u32 = u32::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_pack_unpack_u64() {
    let original: u64 = 0x123456789abcdef0;
    let mut buffer = vec![0u8; 10];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 8);
    // Check little-endian byte order
    assert_eq!(buffer[0], 0xf0);
    assert_eq!(buffer[1], 0xde);
    assert_eq!(buffer[2], 0xbc);
    assert_eq!(buffer[3], 0x9a);
    assert_eq!(buffer[4], 0x78);
    assert_eq!(buffer[5], 0x56);
    assert_eq!(buffer[6], 0x34);
    assert_eq!(buffer[7], 0x12);

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: u64 = u64::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_pack_unpack_u128() {
    let original: u128 = 0x123456789abcdef0fedcba9876543210;
    let mut buffer = vec![0u8; 20];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 16);

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: u128 = u128::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_pack_unpack_i8() {
    let original: i8 = -42;
    let mut buffer = vec![0u8; 10];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 1);

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: i8 = i8::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_pack_unpack_i16() {
    let original: i16 = -12345;
    let mut buffer = vec![0u8; 10];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 2);

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: i16 = i16::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_pack_unpack_i32() {
    let original: i32 = -123456789;
    let mut buffer = vec![0u8; 10];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 4);

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: i32 = i32::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_pack_unpack_i64() {
    let original: i64 = -1234567890123456789;
    let mut buffer = vec![0u8; 10];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 8);

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: i64 = i64::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_pack_unpack_i128() {
    let original: i128 = -123456789012345678901234567890123456789;
    let mut buffer = vec![0u8; 20];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 16);

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: i128 = i128::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_pack_unpack_f32() {
    let original: f32 = 3.14159;
    let mut buffer = vec![0u8; 10];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 4);

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: f32 = f32::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_pack_unpack_f64() {
    let original: f64 = 3.141592653589793;
    let mut buffer = vec![0u8; 10];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 8);

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: f64 = f64::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_pack_unpack_extreme_values() {
    // Test u8 extremes
    for &value in &[u8::MIN, u8::MAX] {
        let mut buffer = vec![0u8; 10];
        let mut cursor = Cursor::new(&mut buffer[..]);
        value.pack(&mut cursor).unwrap();
        let mut read_cursor = Cursor::new(&buffer[..]);
        assert_eq!(u8::unpack(&mut read_cursor).unwrap(), value);
    }

    // Test u16 extremes
    for &value in &[u16::MIN, u16::MAX] {
        let mut buffer = vec![0u8; 10];
        let mut cursor = Cursor::new(&mut buffer[..]);
        value.pack(&mut cursor).unwrap();
        let mut read_cursor = Cursor::new(&buffer[..]);
        assert_eq!(u16::unpack(&mut read_cursor).unwrap(), value);
    }

    // Test u32 extremes
    for &value in &[u32::MIN, u32::MAX] {
        let mut buffer = vec![0u8; 10];
        let mut cursor = Cursor::new(&mut buffer[..]);
        value.pack(&mut cursor).unwrap();
        let mut read_cursor = Cursor::new(&buffer[..]);
        assert_eq!(u32::unpack(&mut read_cursor).unwrap(), value);
    }

    // Test u64 extremes
    for &value in &[u64::MIN, u64::MAX] {
        let mut buffer = vec![0u8; 10];
        let mut cursor = Cursor::new(&mut buffer[..]);
        value.pack(&mut cursor).unwrap();
        let mut read_cursor = Cursor::new(&buffer[..]);
        assert_eq!(u64::unpack(&mut read_cursor).unwrap(), value);
    }
}

#[test]
fn test_pack_unpack_signed_extremes() {
    // Test i8 extremes
    for &value in &[i8::MIN, -1, 0, 1, i8::MAX] {
        let mut buffer = vec![0u8; 10];
        let mut cursor = Cursor::new(&mut buffer[..]);
        value.pack(&mut cursor).unwrap();
        let mut read_cursor = Cursor::new(&buffer[..]);
        assert_eq!(i8::unpack(&mut read_cursor).unwrap(), value);
    }

    // Test i16 extremes
    for &value in &[i16::MIN, -1, 0, 1, i16::MAX] {
        let mut buffer = vec![0u8; 10];
        let mut cursor = Cursor::new(&mut buffer[..]);
        value.pack(&mut cursor).unwrap();
        let mut read_cursor = Cursor::new(&buffer[..]);
        assert_eq!(i16::unpack(&mut read_cursor).unwrap(), value);
    }

    // Test i32 extremes
    for &value in &[i32::MIN, -1, 0, 1, i32::MAX] {
        let mut buffer = vec![0u8; 10];
        let mut cursor = Cursor::new(&mut buffer[..]);
        value.pack(&mut cursor).unwrap();
        let mut read_cursor = Cursor::new(&buffer[..]);
        assert_eq!(i32::unpack(&mut read_cursor).unwrap(), value);
    }

    // Test i64 extremes
    for &value in &[i64::MIN, -1, 0, 1, i64::MAX] {
        let mut buffer = vec![0u8; 10];
        let mut cursor = Cursor::new(&mut buffer[..]);
        value.pack(&mut cursor).unwrap();
        let mut read_cursor = Cursor::new(&buffer[..]);
        assert_eq!(i64::unpack(&mut read_cursor).unwrap(), value);
    }
}

#[test]
fn test_pack_unpack_floating_point_special_values() {
    // Test f32 special values
    for &value in &[f32::NEG_INFINITY, -0.0, 0.0, f32::INFINITY, f32::NAN] {
        let mut buffer = vec![0u8; 10];
        let mut cursor = Cursor::new(&mut buffer[..]);
        value.pack(&mut cursor).unwrap();
        let mut read_cursor = Cursor::new(&buffer[..]);
        let unpacked = f32::unpack(&mut read_cursor).unwrap();
        if value.is_nan() {
            assert!(unpacked.is_nan());
        } else {
            assert_eq!(unpacked, value);
        }
    }

    // Test f64 special values
    for &value in &[f64::NEG_INFINITY, -0.0, 0.0, f64::INFINITY, f64::NAN] {
        let mut buffer = vec![0u8; 10];
        let mut cursor = Cursor::new(&mut buffer[..]);
        value.pack(&mut cursor).unwrap();
        let mut read_cursor = Cursor::new(&buffer[..]);
        let unpacked = f64::unpack(&mut read_cursor).unwrap();
        if value.is_nan() {
            assert!(unpacked.is_nan());
        } else {
            assert_eq!(unpacked, value);
        }
    }
}

#[test]
fn test_pack_multiple_values() {
    let mut buffer = vec![0u8; 100];
    let mut cursor = Cursor::new(&mut buffer[..]);

    let val1: u8 = 42;
    let val2: u16 = 0x1234;
    let val3: u32 = 0x12345678;
    let val4: f32 = 3.14159;

    // Pack multiple values
    let mut total_bytes = 0;
    total_bytes += val1.pack(&mut cursor).unwrap();
    total_bytes += val2.pack(&mut cursor).unwrap();
    total_bytes += val3.pack(&mut cursor).unwrap();
    total_bytes += val4.pack(&mut cursor).unwrap();

    assert_eq!(total_bytes, 1 + 2 + 4 + 4);

    // Unpack multiple values
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked_val1: u8 = u8::unpack(&mut read_cursor).unwrap();
    let unpacked_val2: u16 = u16::unpack(&mut read_cursor).unwrap();
    let unpacked_val3: u32 = u32::unpack(&mut read_cursor).unwrap();
    let unpacked_val4: f32 = f32::unpack(&mut read_cursor).unwrap();

    assert_eq!(unpacked_val1, val1);
    assert_eq!(unpacked_val2, val2);
    assert_eq!(unpacked_val3, val3);
    assert_eq!(unpacked_val4, val4);
}

#[test]
fn test_unpack_insufficient_data() {
    // Try to unpack u32 from buffer with only 2 bytes
    let buffer = vec![0x12, 0x34];
    let mut cursor = Cursor::new(&buffer[..]);

    let result = u32::unpack(&mut cursor);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::ReaderOutOfData => {}
        _ => panic!("Expected ReaderOutOfData error"),
    }
}

#[test]
fn test_pack_insufficient_space() {
    // Try to pack u32 into buffer with only 2 bytes
    let mut buffer = vec![0u8; 2];
    let mut cursor = Cursor::new(&mut buffer[..]);

    let value: u32 = 0x12345678;
    let result = value.pack(&mut cursor);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::WriterOutOfSpace => {}
        _ => panic!("Expected WriterOutOfSpace error"),
    }
}

#[test]
fn test_pack_unpack_with_vec_writer() {
    let mut buffer = Vec::new();

    let val1: u16 = 0x1234;
    let val2: u32 = 0x56789abc;

    // Pack into Vec
    val1.pack(&mut buffer).unwrap();
    val2.pack(&mut buffer).unwrap();

    // Verify the buffer contains the expected bytes
    assert_eq!(buffer.len(), 6);
    assert_eq!(buffer[0], 0x34); // u16 low byte
    assert_eq!(buffer[1], 0x12); // u16 high byte
    assert_eq!(buffer[2], 0xbc); // u32 byte 0
    assert_eq!(buffer[3], 0x9a); // u32 byte 1
    assert_eq!(buffer[4], 0x78); // u32 byte 2
    assert_eq!(buffer[5], 0x56); // u32 byte 3

    // Unpack from the buffer
    let mut cursor = Cursor::new(&buffer[..]);
    let unpacked_val1: u16 = u16::unpack(&mut cursor).unwrap();
    let unpacked_val2: u32 = u32::unpack(&mut cursor).unwrap();

    assert_eq!(unpacked_val1, val1);
    assert_eq!(unpacked_val2, val2);
}

#[test]
fn test_round_trip_consistency() {
    // Test that pack followed by unpack is identity for various types
    fn test_round_trip<T: Pack + PartialEq + core::fmt::Debug + Copy>(value: T) {
        let mut buffer = Vec::new();
        value.pack(&mut buffer).unwrap();
        let mut cursor = Cursor::new(&buffer[..]);
        let unpacked = T::unpack(&mut cursor).unwrap();
        assert_eq!(value, unpacked);
    }

    test_round_trip(0u8);
    test_round_trip(255u8);
    test_round_trip(0u16);
    test_round_trip(65535u16);
    test_round_trip(0u32);
    test_round_trip(4294967295u32);
    test_round_trip(0u64);
    test_round_trip(18446744073709551615u64);

    test_round_trip(-128i8);
    test_round_trip(127i8);
    test_round_trip(-32768i16);
    test_round_trip(32767i16);
    test_round_trip(-2147483648i32);
    test_round_trip(2147483647i32);
    test_round_trip(-9223372036854775808i64);
    test_round_trip(9223372036854775807i64);

    test_round_trip(0.0f32);
    test_round_trip(1.0f32);
    test_round_trip(-1.0f32);
    test_round_trip(f32::MIN);
    test_round_trip(f32::MAX);

    test_round_trip(0.0f64);
    test_round_trip(1.0f64);
    test_round_trip(-1.0f64);
    test_round_trip(f64::MIN);
    test_round_trip(f64::MAX);
}

#[test]
fn test_pack_unpack_usize() {
    let original: usize = 0x123456789abcdef0 as usize;
    let mut buffer = vec![0u8; 20];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, core::mem::size_of::<usize>());

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: usize = usize::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_pack_unpack_isize() {
    let original: isize = -123456789 as isize;
    let mut buffer = vec![0u8; 20];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, core::mem::size_of::<isize>());

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: isize = isize::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

// Array Pack implementation tests
#[test]
fn test_pack_unpack_array_u8() {
    let original: [u8; 4] = [0x12, 0x34, 0x56, 0x78];
    let mut buffer = vec![0u8; 20];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 4);
    assert_eq!(buffer[0], 0x12);
    assert_eq!(buffer[1], 0x34);
    assert_eq!(buffer[2], 0x56);
    assert_eq!(buffer[3], 0x78);

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: [u8; 4] = <[u8; 4]>::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_pack_unpack_array_u16() {
    let original: [u16; 3] = [0x1234, 0x5678, 0x9abc];
    let mut buffer = vec![0u8; 20];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 6);

    // Check little-endian byte order for each u16
    assert_eq!(buffer[0], 0x34); // First u16 low byte
    assert_eq!(buffer[1], 0x12); // First u16 high byte
    assert_eq!(buffer[2], 0x78); // Second u16 low byte
    assert_eq!(buffer[3], 0x56); // Second u16 high byte
    assert_eq!(buffer[4], 0xbc); // Third u16 low byte
    assert_eq!(buffer[5], 0x9a); // Third u16 high byte

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: [u16; 3] = <[u16; 3]>::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_pack_unpack_array_u32() {
    let original: [u32; 2] = [0x12345678, 0x9abcdef0];
    let mut buffer = vec![0u8; 20];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 8);

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: [u32; 2] = <[u32; 2]>::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_pack_unpack_array_mixed_sizes() {
    // Test arrays of different sizes
    let arr1: [u8; 1] = [42];
    let arr5: [u8; 5] = [1, 2, 3, 4, 5];
    let arr10: [u8; 10] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];

    // Test single element array
    let mut buffer = vec![0u8; 20];
    let mut cursor = Cursor::new(&mut buffer[..]);
    let bytes_written = arr1.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 1);
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: [u8; 1] = <[u8; 1]>::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, arr1);

    // Test 5-element array
    let mut buffer = vec![0u8; 20];
    let mut cursor = Cursor::new(&mut buffer[..]);
    let bytes_written = arr5.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 5);
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: [u8; 5] = <[u8; 5]>::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, arr5);

    // Test 10-element array
    let mut buffer = vec![0u8; 20];
    let mut cursor = Cursor::new(&mut buffer[..]);
    let bytes_written = arr10.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 10);
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: [u8; 10] = <[u8; 10]>::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, arr10);
}

#[test]
fn test_pack_unpack_array_floating_point() {
    let original: [f32; 3] = [3.14159, -2.71828, 0.0];
    let mut buffer = vec![0u8; 20];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 12); // 3 * 4 bytes per f32

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: [f32; 3] = <[f32; 3]>::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_pack_unpack_array_signed_integers() {
    let original: [i32; 4] = [-1000000, 0, 1000000, i32::MAX];
    let mut buffer = vec![0u8; 20];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 16); // 4 * 4 bytes per i32

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: [i32; 4] = <[i32; 4]>::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_pack_unpack_empty_array() {
    let original: [u8; 0] = [];
    let mut buffer = vec![0u8; 20];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 0);

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: [u8; 0] = <[u8; 0]>::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_array_pack_with_vec_writer() {
    let original: [u16; 3] = [0x1234, 0x5678, 0x9abc];
    let mut buffer = Vec::new();

    // Pack into Vec
    original.pack(&mut buffer).unwrap();

    // Verify the buffer contains the expected bytes in little-endian order
    assert_eq!(buffer.len(), 6);
    assert_eq!(buffer[0], 0x34); // First u16 low byte
    assert_eq!(buffer[1], 0x12); // First u16 high byte
    assert_eq!(buffer[2], 0x78); // Second u16 low byte
    assert_eq!(buffer[3], 0x56); // Second u16 high byte
    assert_eq!(buffer[4], 0xbc); // Third u16 low byte
    assert_eq!(buffer[5], 0x9a); // Third u16 high byte

    // Unpack from the buffer
    let mut cursor = Cursor::new(&buffer[..]);
    let unpacked: [u16; 3] = <[u16; 3]>::unpack(&mut cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_array_unpack_insufficient_data() {
    // Try to unpack [u32; 2] from buffer with only enough data for one u32
    let buffer = vec![0x12, 0x34, 0x56, 0x78]; // Only 4 bytes, need 8
    let mut cursor = Cursor::new(&buffer[..]);

    let result = <[u32; 2]>::unpack(&mut cursor);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::ReaderOutOfData => {}
        _ => panic!("Expected ReaderOutOfData error"),
    }
}

#[test]
fn test_array_pack_insufficient_space() {
    // Try to pack [u32; 2] into buffer with only 4 bytes
    let original: [u32; 2] = [0x12345678, 0x9abcdef0];
    let mut buffer = vec![0u8; 4]; // Only 4 bytes, need 8
    let mut cursor = Cursor::new(&mut buffer[..]);

    let result = original.pack(&mut cursor);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::WriterOutOfSpace => {}
        _ => panic!("Expected WriterOutOfSpace error"),
    }
}

#[test]
fn test_array_round_trip_consistency() {
    // Test that pack followed by unpack is identity for array types
    fn test_array_round_trip<T: Pack + PartialEq + core::fmt::Debug + Copy, const N: usize>(
        value: [T; N],
    ) {
        let mut buffer = Vec::new();
        value.pack(&mut buffer).unwrap();
        let mut cursor = Cursor::new(&buffer[..]);
        let unpacked = <[T; N]>::unpack(&mut cursor).unwrap();
        assert_eq!(value, unpacked);
    }

    test_array_round_trip([0u8, 255u8, 128u8]);
    test_array_round_trip([u16::MIN, u16::MAX]);
    test_array_round_trip([u32::MIN, 12345u32, u32::MAX]);
    test_array_round_trip([u64::MIN, u64::MAX]);

    test_array_round_trip([i8::MIN, 0i8, i8::MAX]);
    test_array_round_trip([i16::MIN, -1i16, 0i16, 1i16, i16::MAX]);
    test_array_round_trip([i32::MIN, i32::MAX]);
    test_array_round_trip([i64::MIN, i64::MAX]);

    test_array_round_trip([0.0f32, 1.0f32, -1.0f32]);
    test_array_round_trip([f64::MIN, 0.0f64, f64::MAX]);
}

#[test]
fn test_large_array() {
    // Test a larger array to ensure the implementation scales
    let original: [u8; 100] = [42; 100];
    let mut buffer = vec![0u8; 200];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 100);

    // Verify all bytes are 42
    for i in 0..100 {
        assert_eq!(buffer[i], 42);
    }

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: [u8; 100] = <[u8; 100]>::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}

#[test]
fn test_nested_array_concepts() {
    // While we can't easily test truly nested arrays due to type complexity,
    // we can test arrays containing larger elements
    let original: [u64; 8] = [
        0x123456789abcdef0,
        0x0fedcba987654321,
        u64::MIN,
        u64::MAX,
        0,
        1,
        0x5555555555555555,
        0xaaaaaaaaaaaaaaaa,
    ];
    let mut buffer = vec![0u8; 100];
    let mut cursor = Cursor::new(&mut buffer[..]);

    // Test packing
    let bytes_written = original.pack(&mut cursor).unwrap();
    assert_eq!(bytes_written, 64); // 8 * 8 bytes per u64

    // Test unpacking
    let mut read_cursor = Cursor::new(&buffer[..]);
    let unpacked: [u64; 8] = <[u64; 8]>::unpack(&mut read_cursor).unwrap();
    assert_eq!(unpacked, original);
}
