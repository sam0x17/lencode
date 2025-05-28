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

    /// Consumes the [`BitWriter`], returning the underlying writer.
    ///
    /// Useful for scenarios where you are writing directly into memory, such as into a [`Vec<u8>`].
    #[inline(always)]
    pub fn into_inner(mut self) -> Result<W, Error> {
        self.flush()?;
        let writer = self.writer.take().expect("must be defined");
        Ok(writer)
    }

    /// Write a single bit into the buffer.
    #[inline(always)]
    pub fn write_bit(&mut self, bit: bool) -> Result<(), Error> {
        // Flush if buffer full
        if self.cursor == BUFFER_SIZE * 8 {
            self.flush_buffer()?;
        }
        // Set bit
        self.buffer.set(self.cursor, bit);
        self.cursor += 1;
        Ok(())
    }

    /// Flush the internal buffer to the underlying writer.
    #[inline(always)]
    fn flush_buffer(&mut self) -> Result<(), Error> {
        let bytes = (self.cursor + 7) / 8;
        let raw = self.buffer.as_raw_slice();
        let written = self
            .writer
            .as_mut()
            .expect("must be defined")
            .write(&raw[..bytes])?;
        if written != bytes {
            return Err(Error::WriteShort);
        }
        self.cursor = 0;
        // zero out buffer for next use
        // by resetting the slice
        for byte in &mut self.buffer.as_raw_mut_slice()[..bytes] {
            *byte = 0;
        }
        Ok(())
    }

    /// Flush any pending bits (padding the final byte with zeroes) and then the underlying writer.
    #[inline(always)]
    pub fn flush(&mut self) -> Result<(), Error> {
        if self.cursor > 0 {
            self.flush_buffer()?;
        }
        let Some(writer) = self.writer.as_mut() else {
            return Ok(());
        };
        writer.flush()
    }
}

impl<W: Write, const BUFFER_SIZE: usize, Order: BitOrder> Drop
    for BitWriter<W, BUFFER_SIZE, Order>
{
    fn drop(&mut self) {
        // Ensure we flush any remaining bits when the BitWriter is dropped
        let _ = self.flush();
    }
}

#[cfg(all(test, not(feature = "std")))]
extern crate alloc;
#[cfg(all(test, not(feature = "std")))]
use alloc::{vec, vec::Vec};

#[test]
fn test_write_and_read_roundtrip() {
    let data = Vec::new();
    let mut writer = BitWriter::<_, 2, Msb0>::new(data);

    // Write a pattern of bits
    let pattern = [true, false, true, true, false, false, true, false];
    for b in &pattern {
        writer.write_bit(*b).unwrap();
    }
    // flush partial
    writer.flush().unwrap();

    let data = writer.into_inner().unwrap();

    // Should be one byte 0b10110010
    assert_eq!(data, vec![0b1011_0010]);
}

#[test]
fn test_buffer_boundary_flush() {
    let data = Vec::new();
    // small buffer of 1 byte to force automatic flush
    let mut writer = BitWriter::<_, 1, Msb0>::new(data);

    // Write 12 bits: should flush after 8, then write 4 into new byte
    for _ in 0..12 {
        writer.write_bit(true).unwrap();
    }
    writer.flush().unwrap();

    let out = writer.into_inner().unwrap();
    // first byte = 0xFF, second = 0xF0 (4 ones high bits)
    assert_eq!(out, vec![0xFF, 0xF0]);
}

#[test]
fn test_lsb0_writer() {
    let data = Vec::new();
    let mut writer = BitWriter::<_, 2, Lsb0>::new(data);

    // Write bits: least-significant bit first yields reversed byte
    let pattern = [true, false, true, false, true, false, true, false];
    for b in &pattern {
        writer.write_bit(*b).unwrap();
    }
    writer.flush().unwrap();

    let out = writer.into_inner().unwrap();
    // pattern bits form 0b01010101 = 0x55
    assert_eq!(out, vec![0x55]);
}
