use super::{Error, Write};
use bitvec::prelude::*;

/// `BitWriter` writes bits (MSB- or LSB-first) into an underlying `Write` sink.
pub struct BitWriter<W: Write, const BUFFER_SIZE: usize = 64_000, Order: BitOrder = Msb0> {
    writer: Option<W>,
    buffer: BitArray<[u8; BUFFER_SIZE], Order>,
    /// how many bits have been written into `buffer`
    cursor: usize,
}

impl<W: Write, const BUFFER_SIZE: usize, Order: BitOrder> BitWriter<W, BUFFER_SIZE, Order> {
    /// Create a new BitWriter over `writer`.
    #[inline(always)]
    pub fn new(writer: W) -> Self {
        BitWriter {
            writer: Some(writer),
            buffer: BitArray::new([0u8; BUFFER_SIZE]),
            cursor: 0,
        }
    }

    /// Write a single bit into the buffer.
    #[inline(always)]
    pub fn write_bit(&mut self, bit: bool) -> Result<(), Error> {
        // auto-flush if buffer full
        if self.cursor >= BUFFER_SIZE * 8 {
            self.flush_buffer()?;
        }
        // delegate to BitArray which respects bit ordering (Msb0 vs Lsb0)
        self.buffer.set(self.cursor, bit);
        self.cursor += 1;
        Ok(())
    }

    /// Write up to 64 bits (LSB-first within the provided `u64`).
    #[inline(always)]
    pub fn write_bits<const N: usize>(&mut self, mut v: u64) -> Result<(), Error> {
        const {
            assert!(N <= 64, "can write at most 64 bits");
        }
        for _ in 0..N {
            let b = (v & 1) != 0;
            self.write_bit(b)?;
            v >>= 1;
        }
        Ok(())
    }

    /// Consumes the [`BitWriter`], returning the underlying writer.
    #[inline(always)]
    pub fn into_inner(mut self) -> Result<W, Error> {
        self.flush_all()?;
        Ok(self.writer.take().expect("writer missing"))
    }

    /// Flush full bytes in the buffer to the underlying writer.
    #[inline(always)]
    fn flush_buffer(&mut self) -> Result<(), Error> {
        let bytes = self.cursor.div_ceil(8);
        let raw = self.buffer.as_raw_slice();
        let w = self.writer.as_mut().expect("writer missing");
        let written = w.write(&raw[..bytes])?;
        if written != bytes {
            return Err(Error::WriteShort);
        }
        // reset buffer
        self.cursor = 0;
        for byte in &mut self.buffer.as_raw_mut_slice()[..bytes] {
            *byte = 0;
        }
        Ok(())
    }

    /// Flush any pending bits (padding the final byte with zeroes) and underlying writer.
    #[inline(always)]
    pub fn flush_all(&mut self) -> Result<(), Error> {
        if self.cursor > 0 {
            self.flush_buffer()?;
        }
        if let Some(w) = self.writer.as_mut() {
            w.flush()?;
        }
        Ok(())
    }
}

impl<W: Write, const BUFFER_SIZE: usize, Order: BitOrder> Drop
    for BitWriter<W, BUFFER_SIZE, Order>
{
    fn drop(&mut self) {
        let _ = self.flush_all();
    }
}

/// `Write` impl for MSB-first ordering
impl<W: Write, const BUFFER_SIZE: usize> Write for BitWriter<W, BUFFER_SIZE, Msb0> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        let mut written = 0;
        for &byte in buf {
            // determine where to insert next
            let bit_offset = self.cursor & 7;
            let mut byte_idx = self.cursor >> 3;
            if byte_idx >= BUFFER_SIZE {
                self.flush_buffer()?;
                byte_idx = self.cursor >> 3;
            }
            // aligned vs misaligned split for MSB0
            if bit_offset == 0 {
                // aligned
                self.buffer.as_raw_mut_slice()[byte_idx] = byte;
            } else {
                // misaligned: high bits into current, low bits into next
                let raw = self.buffer.as_raw_mut_slice();
                raw[byte_idx] |= byte >> bit_offset;
                byte_idx += 1;
                if byte_idx >= BUFFER_SIZE {
                    self.flush_buffer()?;
                    byte_idx = self.cursor >> (3 + 1);
                }
                let raw = self.buffer.as_raw_mut_slice();
                raw[byte_idx] |= byte << (8 - bit_offset);
            }
            self.cursor += 8;
            written += 1;
        }
        Ok(written)
    }

    #[inline(always)]
    fn flush(&mut self) -> Result<(), Error> {
        self.flush_all()
    }
}

/// `Write` impl for LSB-first ordering
impl<W: Write, const BUFFER_SIZE: usize> Write for BitWriter<W, BUFFER_SIZE, Lsb0> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        let mut written = 0;
        for &byte in buf {
            let bit_offset = self.cursor & 7;
            let mut byte_idx = self.cursor >> 3;
            if byte_idx >= BUFFER_SIZE {
                self.flush_buffer()?;
                byte_idx = self.cursor >> 3;
            }
            if bit_offset == 0 {
                // aligned: store reversed bits so LSB-first
                self.buffer.as_raw_mut_slice()[byte_idx] = byte.reverse_bits();
            } else {
                // misaligned: split reversed
                let rev = byte.reverse_bits();
                let raw = self.buffer.as_raw_mut_slice();
                raw[byte_idx] |= rev << bit_offset;
                byte_idx += 1;
                if byte_idx >= BUFFER_SIZE {
                    self.flush_buffer()?;
                    byte_idx = self.cursor >> (3 + 1);
                }
                let raw = self.buffer.as_raw_mut_slice();
                raw[byte_idx] |= rev >> (8 - bit_offset);
            }
            self.cursor += 8;
            written += 1;
        }
        Ok(written)
    }

    #[inline(always)]
    fn flush(&mut self) -> Result<(), Error> {
        self.flush_all()
    }
}

#[cfg(all(test, not(feature = "std")))]
extern crate alloc;
#[cfg(all(test, not(feature = "std")))]
use alloc::{vec, vec::Vec};

#[test]
fn test_write_and_read_roundtrip() {
    let mut writer = BitWriter::<_, 2, Msb0>::new(Vec::new());
    writer.write(&[0b1011_0010]).unwrap();
    writer.flush().unwrap();
    let out = writer.into_inner().unwrap();
    assert_eq!(out, vec![0b1011_0010]);
}

#[test]
fn test_buffer_boundary_flush() {
    let mut writer = BitWriter::<_, 1, Msb0>::new(Vec::new());
    let buf = vec![0xFF; 12];
    writer.write(&buf).unwrap();
    writer.flush().unwrap();
    let out = writer.into_inner().unwrap();
    assert_eq!(out, buf);
}

#[test]
fn test_write_misaligned() {
    let mut writer = BitWriter::<_, 2, Msb0>::new(Vec::new());
    // prefill 4 bits: high nibble '1111'
    for _ in 0..4 {
        writer.write_bit(true).unwrap();
    }
    // write byte 0xAB at current bit offset (4)
    writer.write(&[0xAB]).unwrap();
    writer.flush().unwrap();
    let out = writer.into_inner().unwrap();
    // first byte = 0xF0 | (0xAB >> 4) = 0xFA
    // second byte = (0xAB & 0x0F) << 4 = 0xB0
    assert_eq!(out, vec![0xFA, 0xB0]);
}

#[test]
fn test_lsb0_writer() {
    let mut writer = BitWriter::<_, 2, Lsb0>::new(Vec::new());
    writer.write(&[0xAA]).unwrap();
    writer.flush().unwrap();
    let out = writer.into_inner().unwrap();
    assert_eq!(out, vec![0x55]);
}

#[test]
fn test_write_bits_msb0() {
    let mut writer = BitWriter::<_, 2, Msb0>::new(Vec::new());
    writer.write_bits::<12>(0xABC).unwrap();
    writer.flush().unwrap();
    let out = writer.into_inner().unwrap();
    // 0xABC LSB-first → bits [0,0,1,1,1,1,0,1,0,1,0,1]
    // MSB0 storage → raw bytes [0x3D, 0x50]
    assert_eq!(out, vec![0x3D, 0x50]);
}

#[test]
fn test_write_bits_lsb0() {
    let mut writer = BitWriter::<_, 2, Lsb0>::new(Vec::new());
    writer.write_bits::<12>(0xABC).unwrap();
    writer.flush().unwrap();
    let out = writer.into_inner().unwrap();
    // LSB0 storage → raw bytes [0xBC, 0x0A]
    assert_eq!(out, vec![0xBC, 0x0A]);
}
