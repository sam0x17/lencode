use super::{Error, Write};
use crate::*;
use bitvec::prelude::*;

/// `BitWriter` writes bits (MSB- or LSB-first) into an underlying `Write` sink.
pub struct BitWriter<W: Write, O: BitOrder = Msb0, const N: usize = 256> {
    writer: Option<W>,
    buffer: BitArray<[u8; N], O>,
    /// how many bits have been written into `buffer`
    cursor: usize,
}

impl<W: Write, O: BitOrder, const N: usize> BitWriter<W, O, N> {
    /// Create a new BitWriter over `writer`.
    #[inline(always)]
    pub fn new(writer: W) -> Self {
        BitWriter {
            writer: Some(writer),
            buffer: BitArray::new([0u8; N]),
            cursor: 0,
        }
    }

    /// Write a single bit into the buffer.
    #[inline(always)]
    pub fn write_bit(&mut self, bit: bool) -> Result<()> {
        // auto-flush if buffer full
        if self.cursor >= N * 8 {
            self.flush_buffer()?;
        }
        // delegate to BitArray which respects bit ordering (Msb0 vs Lsb0)
        self.buffer.set(self.cursor, bit);
        self.cursor += 1;
        Ok(())
    }

    /// Write up to 64 bits (LSB-first within the provided `u64`).
    ///
    /// This implementation mirrors [`Write::write`], avoiding per-bit calls by
    /// emitting whole bytes whenever possible and falling back to at most seven
    /// trailing bit writes. This drastically reduces the number of `write_bit`
    /// invocations when large values are written.
    #[inline(always)]
    pub fn write_bits<const NUM: u8>(&mut self, v: u64) -> Result<()>
    where
        Self: Write,
    {
        const {
            assert!(NUM <= 64, "can write at most 64 bits");
        }

        // fast path for whole bytes
        let full_bytes = (NUM / 8) as usize;
        let rem_bits = (NUM % 8) as usize;
        let bytes = v.to_le_bytes();

        for &byte in &bytes[..full_bytes] {
            let b = byte.reverse_bits();
            <Self as Write>::write(self, &[b])?;
        }

        if rem_bits > 0 {
            let tail = (v >> (full_bytes * 8)) as u8;
            for j in 0..rem_bits {
                let bit = (tail >> j) & 1 != 0;
                self.write_bit(bit)?;
            }
        }

        Ok(())
    }

    /// Consumes the [`BitWriter`], returning the underlying writer.
    #[inline(always)]
    pub fn into_inner(mut self) -> Result<W> {
        self.flush_all()?;
        Ok(self.writer.take().expect("writer missing"))
    }

    /// Flush full bytes in the buffer to the underlying writer.
    #[inline(always)]
    fn flush_buffer(&mut self) -> Result<()> {
        let bytes = self.cursor.div_ceil(8);
        let raw = self.buffer.as_raw_slice();
        let w = self.writer.as_mut().expect("writer missing");
        let written = w.write(&raw[..bytes])?;
        if written != bytes {
            return Err(Error::WriteShort);
        }
        // reset buffer
        self.cursor = 0;
        self.buffer.as_raw_mut_slice()[..bytes].fill(0);
        Ok(())
    }

    /// Flush any pending bits (padding the final byte with zeroes) and underlying writer.
    #[inline(always)]
    pub fn flush_all(&mut self) -> Result<()> {
        if self.cursor > 0 {
            self.flush_buffer()?;
        }
        if let Some(w) = self.writer.as_mut() {
            w.flush()?;
        }
        Ok(())
    }
}

impl<W: Write, O: BitOrder, const N: usize> Drop for BitWriter<W, O, N> {
    #[inline(always)]
    fn drop(&mut self) {
        let _ = self.flush_all();
    }
}

/// `Write` impl for MSB-first ordering
impl<W: Write, const N: usize> Write for BitWriter<W, Msb0, N> {
    #[inline(always)]
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        for &byte in buf {
            let bit_offset = self.cursor & 7;
            let mut byte_idx = self.cursor >> 3;

            if bit_offset == 0 {
                if byte_idx >= N {
                    self.flush_buffer()?;
                    byte_idx = 0;
                }
                self.buffer.as_raw_mut_slice()[byte_idx] = byte;
                self.cursor += 8;
            } else {
                if byte_idx >= N {
                    self.flush_buffer()?;
                    byte_idx = 0;
                }
                {
                    let raw = self.buffer.as_raw_mut_slice();
                    raw[byte_idx] |= byte >> bit_offset;
                }
                byte_idx += 1;
                if byte_idx >= N {
                    self.flush_buffer()?;
                    byte_idx = 0;
                    let raw = self.buffer.as_raw_mut_slice();
                    raw[byte_idx] |= byte << (8 - bit_offset);
                    self.cursor = bit_offset;
                } else {
                    let raw = self.buffer.as_raw_mut_slice();
                    raw[byte_idx] |= byte << (8 - bit_offset);
                    self.cursor += 8;
                }
            }
        }
        Ok(buf.len())
    }

    #[inline(always)]
    fn flush(&mut self) -> Result<()> {
        self.flush_all()
    }
}

/// `Write` impl for LSB-first ordering
impl<W: Write, const N: usize> Write for BitWriter<W, Lsb0, N> {
    #[inline(always)]
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        for &byte in buf {
            let bit_offset = self.cursor & 7;
            let mut byte_idx = self.cursor >> 3;

            if bit_offset == 0 {
                if byte_idx >= N {
                    self.flush_buffer()?;
                    byte_idx = 0;
                }
                self.buffer.as_raw_mut_slice()[byte_idx] = byte.reverse_bits();
                self.cursor += 8;
            } else {
                if byte_idx >= N {
                    self.flush_buffer()?;
                    byte_idx = 0;
                }
                let rev = byte.reverse_bits();
                {
                    let raw = self.buffer.as_raw_mut_slice();
                    raw[byte_idx] |= rev << bit_offset;
                }
                byte_idx += 1;
                if byte_idx >= N {
                    self.flush_buffer()?;
                    byte_idx = 0;
                    let raw = self.buffer.as_raw_mut_slice();
                    raw[byte_idx] |= rev >> (8 - bit_offset);
                    self.cursor = bit_offset;
                } else {
                    let raw = self.buffer.as_raw_mut_slice();
                    raw[byte_idx] |= rev >> (8 - bit_offset);
                    self.cursor += 8;
                }
            }
        }
        Ok(buf.len())
    }

    #[inline(always)]
    fn flush(&mut self) -> Result<()> {
        self.flush_all()
    }
}

#[test]
fn test_write_and_read_roundtrip() {
    let mut writer = BitWriter::<_, Msb0, 2>::new(Vec::new());
    writer.write(&[0b1011_0010]).unwrap();
    writer.flush().unwrap();
    let out = writer.into_inner().unwrap();
    assert_eq!(out, vec![0b1011_0010]);
}

#[test]
fn test_buffer_boundary_flush() {
    let mut writer = BitWriter::<_, Msb0, 1>::new(Vec::new());
    let buf = vec![0xFF; 12];
    writer.write(&buf).unwrap();
    writer.flush().unwrap();
    let out = writer.into_inner().unwrap();
    assert_eq!(out, buf);
}

#[test]
fn test_write_misaligned() {
    let mut writer = BitWriter::<_, Msb0, 2>::new(Vec::new());
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
    let mut writer = BitWriter::<_, Lsb0, 2>::new(Vec::new());
    writer.write(&[0xAA]).unwrap();
    writer.flush().unwrap();
    let out = writer.into_inner().unwrap();
    assert_eq!(out, vec![0x55]);
}

#[test]
fn test_write_bits_msb0() {
    let mut writer = BitWriter::<_, Msb0, 2>::new(Vec::new());
    writer.write_bits::<12>(0xABC).unwrap();
    writer.flush().unwrap();
    let out = writer.into_inner().unwrap();
    // 0xABC LSB-first → bits [0,0,1,1,1,1,0,1,0,1,0,1]
    // MSB0 storage → raw bytes [0x3D, 0x50]
    assert_eq!(out, vec![0x3D, 0x50]);
}

#[test]
fn test_write_bits_lsb0() {
    let mut writer = BitWriter::<_, Lsb0, 2>::new(Vec::new());
    writer.write_bits::<12>(0xABC).unwrap();
    writer.flush().unwrap();
    let out = writer.into_inner().unwrap();
    // LSB0 storage → raw bytes [0xBC, 0x0A]
    assert_eq!(out, vec![0xBC, 0x0A]);
}

#[test]
fn test_write_unaligned_msb0_small_buffer_edge_case() {
    let mut writer = BitWriter::<_, Msb0, 1>::new(Vec::new());
    // write 4 bits: 0b1100
    for bit in [true, true, false, false] {
        writer.write_bit(bit).unwrap();
    }
    writer.write(&[0b10101010, 0b01010101]).unwrap();
    writer.write_bit(false).unwrap();
    writer.write_bit(false).unwrap();
    writer.write_bit(false).unwrap();
    writer.write_bit(false).unwrap();
    let out = writer.into_inner().unwrap();
    assert_eq!(out, vec![0b1100_1010, 0b1010_0101, 0b0101_0000]);
}
