use super::{Error, Read};
use bitvec::prelude::{BitArray, Msb0};

pub struct BitReader<R: Read, const BUFFER_SIZE: usize = 64_000> {
    reader: R,
    buffer: BitArray<[u8; BUFFER_SIZE], Msb0>,
    filled: usize, // how many bytes of `buffer` are valid
    cursor: usize, // next bit position [0..filled*8)
}

impl<R: Read, const BUFFER_SIZE: usize> BitReader<R, BUFFER_SIZE> {
    pub fn new(reader: R) -> Self {
        BitReader {
            reader,
            buffer: BitArray::new([0u8; BUFFER_SIZE]),
            filled: 0,
            cursor: 0,
        }
    }

    fn fill_buffer(&mut self) -> Result<(), Error> {
        let total_bits = self.filled * 8;
        let bits_remaining = total_bits.saturating_sub(self.cursor);

        // 1) Slide every unread bit down to the front
        for i in 0..bits_remaining {
            let b = self.buffer[self.cursor + i];
            self.buffer.set(i, b);
        }

        // 2) Zero out the now-free tail of the backing bytes
        let raw = self.buffer.as_raw_mut_slice();
        let bytes_remaining = bits_remaining / 8;
        let bit_offset = bits_remaining % 8;
        for byte in &mut raw[bytes_remaining..] {
            *byte = 0;
        }

        // 3) Read fresh data directly into a small temp
        let mut tmp = [0u8; BUFFER_SIZE];
        let bytes_read = self.reader.read(&mut tmp)?;
        if bytes_read == 0 {
            return Err(Error::EndOfData);
        }

        // 4) Splice it into `raw` at the right bit-offset (MSB-first!)
        if bit_offset == 0 {
            raw[bytes_remaining..bytes_remaining + bytes_read].copy_from_slice(&tmp[..bytes_read]);
        } else {
            for i in 0..bytes_read {
                let byte = tmp[i];
                let dst = bytes_remaining + i;
                // New byte’s MSB → raw[dst] bit (7 − bit_offset)
                raw[dst] |= byte >> bit_offset;
                // The “leftover” LSBs carry into the next byte’s MSB side
                if dst + 1 < BUFFER_SIZE {
                    raw[dst + 1] |= byte << (8 - bit_offset);
                }
            }
        }

        // 5) Recompute valid bytes & reset cursor
        let new_total_bits = bits_remaining + bytes_read * 8;
        self.filled = (new_total_bits + 7) / 8;
        self.cursor = 0;
        Ok(())
    }

    pub fn read_bit(&mut self) -> Result<bool, Error> {
        if self.cursor >= self.filled * 8 {
            self.fill_buffer()?;
        }
        if self.cursor >= self.filled * 8 {
            return Err(Error::EndOfData);
        }
        let bit = self.buffer[self.cursor];
        self.cursor += 1;
        Ok(bit)
    }

    pub fn peek_bit(&mut self) -> Result<bool, Error> {
        if self.cursor >= self.filled * 8 {
            self.fill_buffer()?;
        }
        if self.cursor >= self.filled * 8 {
            return Err(Error::EndOfData);
        }
        Ok(self.buffer[self.cursor])
    }
}

impl<R: Read, const BUFFER_SIZE: usize> Read for BitReader<R, BUFFER_SIZE> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        // Enforce byte-alignment or auto-align here if you prefer...
        if self.cursor / 8 >= self.filled {
            self.fill_buffer()?;
        }

        let mut written = 0;
        let raw = self.buffer.as_raw_slice();
        while written < buf.len() && (self.cursor / 8) < self.filled {
            buf[written] = raw[self.cursor / 8];
            self.cursor += 8;
            written += 1;
        }
        Ok(written)
    }
}

#[cfg(all(test, not(feature = "std")))]
extern crate alloc;
#[cfg(all(test, not(feature = "std")))]
use alloc::{vec, vec::Vec};

#[cfg(all(test, not(feature = "std")))]
use crate::io::Cursor;
#[cfg(all(test, feature = "std"))]
use std::io::Cursor;

#[test]
fn test_read_bit_msb0_single_byte() {
    // 0b1010_1101 → bits: 1,0,1,0,1,1,0,1 (MSB-first)
    let data = vec![0b1010_1101];
    let mut br = BitReader::<_, 1>::new(Cursor::new(data));

    let expected = [true, false, true, false, true, true, false, true];
    for &exp in &expected {
        assert_eq!(br.read_bit().unwrap(), exp);
    }

    // now we should be at EOF
    assert!(matches!(br.read_bit(), Err(Error::EndOfData)));
}

#[test]
fn test_peek_bit_does_not_advance() {
    let data = vec![0b1100_0000];
    let mut br = BitReader::<_, 1>::new(Cursor::new(data));

    // peek twice, still the same
    assert_eq!(br.peek_bit().unwrap(), true);
    assert_eq!(br.peek_bit().unwrap(), true);

    // now consume
    assert_eq!(br.read_bit().unwrap(), true);
    // next bit is still the second MSB:
    assert_eq!(br.read_bit().unwrap(), true);
    assert_eq!(br.read_bit().unwrap(), false);
    assert_eq!(br.read_bit().unwrap(), false);
    assert_eq!(br.read_bit().unwrap(), false);
    assert_eq!(br.read_bit().unwrap(), false);
    assert_eq!(br.read_bit().unwrap(), false);
    assert_eq!(br.read_bit().unwrap(), false);

    // now we should be at EOF
    assert!(matches!(br.read_bit(), Err(Error::EndOfData)));
}

#[test]
fn test_fill_and_read_across_buffer_boundary() {
    // Force a refill after 1 byte
    let data = vec![0b1111_0000, 0b0000_1111];
    let mut br = BitReader::<_, 1>::new(Cursor::new(data));

    // Read 12 bits total
    let mut bits = Vec::new();
    for _ in 0..12 {
        bits.push(br.read_bit().unwrap());
    }

    // first byte: 1,1,1,1,0,0,0,0
    // next 4 bits (from second byte): 0,0,0,0
    let expected = [
        true, true, true, true, false, false, false, false, false, false, false, false,
    ];
    assert_eq!(bits, expected);
}

#[test]
fn test_read_bytes_after_bits() {
    let data = vec![0xAB, 0xCD];
    let mut br = BitReader::<_, 2>::new(Cursor::new(data.clone()));

    // consume 4 bits (misaligned)
    for _ in 0..4 {
        br.read_bit().unwrap();
    }

    // now read whole bytes
    let mut buf = [0u8; 1];
    assert_eq!(br.read(&mut buf).unwrap(), 1);
    // should get the first byte (0xAB)
    assert_eq!(buf[0], 0xAB);

    assert_eq!(br.read(&mut buf).unwrap(), 1);
    assert_eq!(buf[0], 0xCD);

    // then EOF
    assert!(matches!(br.read(&mut buf), Err(Error::EndOfData)));
}
