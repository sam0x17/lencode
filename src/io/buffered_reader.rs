// src/io/buffered_reader.rs
use super::*;
use core::cmp::min;

#[cfg(not(any(feature = "std", test)))]
extern crate alloc;
#[cfg(not(any(feature = "std", test)))]
use alloc::vec::Vec;

pub struct BufferedReader<R: Read> {
    reader: R,
    buffer: Vec<u8>,
    producer_pos: usize, // how many bytes in `buffer` are valid
    consumer_pos: usize, // next unread byte in `buffer`
    position: usize,     // total bytes returned so far
}

impl<R: Read + Default> Default for BufferedReader<R> {
    fn default() -> Self {
        // still uses a default capacity for the initial buffer size,
        // but we don't store it permanently on the struct.
        Self::with_capacity(Default::default(), 8 * 1024)
    }
}

impl<R: Read> BufferedReader<R> {
    /// Create with default 8 KiB buffer
    pub fn new(reader: R) -> Self {
        Self::with_capacity(reader, 8 * 1024)
    }

    /// Create with custom buffer size
    pub fn with_capacity(reader: R, capacity: usize) -> Self {
        let mut buffer = Vec::with_capacity(capacity);
        buffer.resize(capacity, 0);
        BufferedReader {
            reader,
            buffer,
            producer_pos: 0,
            consumer_pos: 0,
            position: 0,
        }
    }

    /// Refill the entire buffer from the underlying `reader`
    fn refill(&mut self) -> Result<(), Error> {
        let n = self.reader.read(&mut self.buffer[..])?;
        self.producer_pos = n;
        self.consumer_pos = 0;
        Ok(())
    }

    pub fn read(&mut self, out: &mut [u8]) -> Result<usize, Error> {
        // If our buffer is exhausted, do one refill
        if self.consumer_pos >= self.producer_pos {
            self.refill()?;
            // EOF
            if self.producer_pos == 0 {
                return Ok(0);
            }
        }

        // Copy at most what's buffered or what the caller asked for—no looping
        let avail = self.producer_pos - self.consumer_pos;
        let to_copy = min(avail, out.len());

        out[..to_copy]
            .copy_from_slice(&self.buffer[self.consumer_pos..self.consumer_pos + to_copy]);

        self.consumer_pos += to_copy;
        self.position += to_copy;
        Ok(to_copy)
    }

    pub fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), Error> {
        let mut offset = 0;
        let len = buf.len();
        while offset < len {
            let n = self.read(&mut buf[offset..])?;
            if n == 0 {
                return Err(Error::EndOfData);
            }
            offset += n;
        }
        Ok(())
    }

    /// Total bytes returned so far
    pub fn position(&self) -> usize {
        self.position
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffered_reader() {
        let data = b"Hello, world!";
        let mut reader = BufferedReader::with_capacity(data.as_ref(), 5);

        // 1) First read should pull exactly 5 bytes (“Hello”)
        let mut buf1 = [0; 5];
        let n1 = reader.read(&mut buf1).unwrap();
        assert_eq!(n1, 5);
        assert_eq!(&buf1, b"Hello");
        assert_eq!(reader.position(), 5);

        // 2) Second read asks for 8 bytes, but our buffer is only 5,
        //    so we get at most 5 back in a single call
        let mut buf2 = [0; 8];
        let n2 = reader.read(&mut buf2).unwrap();
        assert_eq!(n2, 5);
        assert_eq!(&buf2[..n2], b", wor");
        assert_eq!(reader.position(), 10);

        // 3) A third read will return the remaining 3 bytes
        let mut buf3 = [0; 3];
        let n3 = reader.read(&mut buf3).unwrap();
        assert_eq!(n3, 3);
        assert_eq!(&buf3, b"ld!");
        assert_eq!(reader.position(), 13);
    }
}
