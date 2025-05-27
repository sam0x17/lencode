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
        // 1) How many bits are still unread?
        let total_bits = self.filled * 8;
        let bits_remaining = total_bits.saturating_sub(self.cursor);

        // 2) Slide leftover bytes *in-place* (byte-aligned):
        let raw = self.buffer.as_raw_mut_slice();
        let start_byte = self.cursor / 8;
        let bytes_remaining = bits_remaining / 8;
        let bit_offset = bits_remaining % 8;

        if bit_offset == 0 {
            // simple: move [start_byte..self.filled) → [0..bytes_remaining)
            raw.copy_within(start_byte..self.filled, 0);
        } else {
            // move whole bytes first (we’ll fix the partial byte in a moment)
            raw.copy_within((start_byte + 1)..self.filled, 1);
            raw[0] = 0; // will OR in the leftover bits below
        }

        // 3) Zero out the tail so new data ORs cleanly
        for b in &mut raw[bytes_remaining..] {
            *b = 0;
        }

        // 4) Read *straight into* the freed region
        let dest = &mut raw[bytes_remaining..];
        let bytes_read = self.reader.read(dest)?;
        if bytes_read == 0 {
            return Err(Error::EndOfData);
        }

        // 5) If we were mid-byte, rotate each newly-read byte
        if bit_offset != 0 {
            let mut carry = 0u8;
            for i in 0..bytes_read {
                let b = dest[i];
                // high (8 - bit_offset) bits move into low bits of carry
                let new_carry = b >> (8 - bit_offset);
                // shift this byte up by bit_offset, OR in the previous carry
                dest[i] = (b << bit_offset) | carry;
                carry = new_carry;
            }
            // tuck the final carry bit into the next raw byte if there is one
            if bytes_remaining + bytes_read < BUFFER_SIZE {
                raw[bytes_remaining + bytes_read] |= carry;
            }
        }

        // 6) Recompute how many bytes are valid now, reset cursor
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
        let mut written = 0;

        while written < buf.len() {
            // 1) make sure we have 8 bits available
            if self.cursor + 8 > self.filled * 8 {
                self.fill_buffer()?;
                if self.cursor + 8 > self.filled * 8 {
                    return Err(Error::EndOfData);
                }
            }

            // 2) now borrow raw only *after* any mutable calls
            let raw = self.buffer.as_raw_slice();
            let bit_offset = self.cursor % 8;
            let byte_idx = self.cursor / 8;

            // 3) extract one aligned or misaligned byte
            let b = if bit_offset == 0 {
                raw[byte_idx]
            } else {
                let hi = raw[byte_idx];
                let lo = if byte_idx + 1 < self.filled {
                    raw[byte_idx + 1]
                } else {
                    0
                };
                // MSB0: high bits of hi shifted left, plus high bits of lo shifted right
                (hi << bit_offset) | (lo >> (8 - bit_offset))
            };

            // 4) write and advance
            buf[written] = b;
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
    let mut br = BitReader::<_, 2>::new(Cursor::new(data));

    // consume 4 bits (misaligned)
    for _ in 0..4 {
        br.read_bit().unwrap();
    }

    // Now the first full byte at bit-offset 4 comes from
    // (0xAB << 4) | (0xCD >> 4) = 0xBC
    let mut buf = [0u8; 1];
    assert_eq!(br.read(&mut buf).unwrap(), 1);
    assert_eq!(buf[0], 0xBC);

    // No more whole bytes left → EOF
    assert!(matches!(br.read(&mut buf), Err(Error::EndOfData)));

    // Now consume the remaining 4 bits (low nibble of 0xCD = 0xD → 1101)
    let mut tail = Vec::new();
    while let Ok(bit) = br.read_bit() {
        tail.push(bit);
    }
    assert_eq!(tail, [true, true, false, true]);
}

#[test]
fn test_empty_input_errors() {
    let data = Vec::<u8>::new();
    let mut br = BitReader::<_, 1>::new(Cursor::new(data));

    // reading any bit immediately errors
    assert!(matches!(br.read_bit(), Err(Error::EndOfData)));
    assert!(matches!(br.peek_bit(), Err(Error::EndOfData)));

    // reading bytes also errors (because fill_buffer finds zero bytes)
    let mut buf = [0u8; 4];
    assert!(matches!(br.read(&mut buf), Err(Error::EndOfData)));
}

#[test]
fn test_exact_buffer_refill() {
    // Create a data stream just over one BUFFER_SIZE so we force multiple refills.
    // Use BUFFER_SIZE=2 to keep this small.
    let data = vec![0b1010_1010, 0b0101_0101, 0b1111_0000];
    let mut br = BitReader::<_, 2>::new(Cursor::new(data.clone()));

    // Read all bits in sequence
    let mut bits = Vec::new();
    while let Ok(b) = br.read_bit() {
        bits.push(b);
    }
    // Should be 3 bytes * 8 = 24 bits
    assert_eq!(bits.len(), 24);

    // Reconstruct bytes MSB0→u8:
    let mut out = Vec::new();
    let mut cursor = 0;
    while cursor + 8 <= bits.len() {
        let byte = bits[cursor..cursor + 8]
            .iter()
            .fold(0u8, |acc, &b| (acc << 1) | (b as u8));
        out.push(byte);
        cursor += 8;
    }
    assert_eq!(out, data);
}

#[test]
fn test_mixed_bit_and_byte_reads_misaligned() {
    let data = vec![0xF0, 0x0F, 0xAA];
    let mut br = BitReader::<_, 3>::new(Cursor::new(data.clone()));

    // Read 3 bits: 1,1,1
    for expected in [true, true, true] {
        assert_eq!(br.read_bit().unwrap(), expected);
    }

    // now read two bytes at bit-offset 3:
    // first = (0xF0 << 3) | (0x0F >> 5) = 0x80
    // second = (0x0F << 3) | (0xAA >> 5) = 0x7D
    let mut buf = [0u8; 2];
    let n = br.read(&mut buf).unwrap();
    assert_eq!(n, 2);
    assert_eq!(buf, [0x80, 0x7D]);

    // advance the three bits we consumed + 16 bits from read = 19 bits
    // total bits = 24, so 5 bits remain: from last byte 0xAA
    let mut tail_bits = Vec::new();
    while let Ok(b) = br.read_bit() {
        tail_bits.push(b);
    }
    let expected_tail = [false, true, false, true, false];
    assert_eq!(tail_bits, expected_tail);
}
