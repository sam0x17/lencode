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

    #[test]
    fn test_read_zero_length_out() {
        let data = b"abc";
        let mut reader = BufferedReader::with_capacity(data.as_ref(), 2);
        let mut buf = [];
        let n = reader.read(&mut buf).unwrap();
        assert_eq!(n, 0);
        assert_eq!(reader.position(), 0);
    }

    #[test]
    fn test_read_eof_on_empty_source() {
        let data: &[u8] = b"";
        let mut reader = BufferedReader::with_capacity(data, 4);
        let mut buf = [0u8; 5];
        let n = reader.read(&mut buf).unwrap();
        assert_eq!(n, 0);
        assert_eq!(reader.position(), 0);
    }

    #[test]
    fn test_read_after_eof_returns_zero() {
        let data = b"HI";
        let mut reader = BufferedReader::with_capacity(data.as_ref(), 2);

        let mut buf = [0u8; 2];
        assert_eq!(reader.read(&mut buf).unwrap(), 2);
        // now at EOF
        assert_eq!(reader.read(&mut buf).unwrap(), 0);
        assert_eq!(reader.position(), 2);
    }

    #[test]
    fn test_small_chunk_reads_until_eof() {
        let data = b"ABCDEFG";
        let mut reader = BufferedReader::with_capacity(data.as_ref(), 2);
        let mut out = Vec::new();

        loop {
            let mut buf = [0u8; 3]; // request bigger than capacity
            let n = reader.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }

        assert_eq!(out, data);
        assert_eq!(reader.position(), data.len());
    }

    #[test]
    fn test_exact_multiple_segments() {
        let data = b"1234567890";
        let mut reader = BufferedReader::with_capacity(data.as_ref(), 4);

        let mut a = [0u8; 3];
        reader.read_exact(&mut a).unwrap();
        assert_eq!(&a, b"123");

        let mut b = [0u8; 4];
        reader.read_exact(&mut b).unwrap();
        assert_eq!(&b, b"4567");

        let mut c = [0u8; 3];
        reader.read_exact(&mut c).unwrap();
        assert_eq!(&c, b"890");

        assert_eq!(reader.position(), 10);
    }

    #[test]
    fn test_read_exact_fails_on_short_source() {
        let data = b"abcd";
        let mut reader = BufferedReader::with_capacity(data.as_ref(), 3);

        // ask for more than available
        let mut buf = [0u8; 6];
        let err = reader.read_exact(&mut buf).unwrap_err();
        assert!(matches!(err, Error::EndOfData));

        // we should have consumed all the source before erroring
        // first chunk: 3, second chunk: 1, then EOF
        assert_eq!(reader.position(), 4);
    }

    #[test]
    fn test_read_exact_zero_length_out() {
        let data = b"XYZ";
        let mut reader = BufferedReader::new(data.as_ref());
        let mut buf = [];
        // nothing to read, but should not error
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(reader.position(), 0);
    }

    #[test]
    fn test_large_read_exact() {
        // generate 10 000 bytes of pseudo–random data
        let size = 10_000;
        let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
        let mut reader = BufferedReader::with_capacity(data.as_slice(), 128);

        let mut buf = vec![0u8; size];
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(buf, data);
        assert_eq!(reader.position(), size);
    }

    #[test]
    fn test_zero_capacity_buffer() {
        let data = b"hello";
        let mut reader = BufferedReader::with_capacity(data.as_ref(), 0);

        // any read should immediately return EOF
        let mut buf = [0u8; 3];
        assert_eq!(reader.read(&mut buf).unwrap(), 0);
        assert_eq!(reader.position(), 0);

        // read_exact must error
        let err = reader.read_exact(&mut buf).unwrap_err();
        assert!(matches!(err, Error::EndOfData));
        assert_eq!(reader.position(), 0);
    }

    /// A reader that always errors.
    struct ErrReader;
    impl Read for ErrReader {
        fn read(&mut self, _buf: &mut [u8]) -> Result<usize, Error> {
            Err(Error::InvalidData)
        }
    }

    #[test]
    fn test_read_single_bytes() {
        let data = b"ABCDE";
        let mut reader = BufferedReader::with_capacity(data.as_ref(), 3);
        let mut out = Vec::new();
        for _ in 0..6 {
            let mut byte = [0u8; 1];
            let n = reader.read(&mut byte).unwrap();
            if n == 0 {
                break;
            }
            out.push(byte[0]);
        }
        assert_eq!(out, b"ABCDE");
        assert_eq!(reader.position(), 5);
        // one more call is EOF
        assert_eq!(reader.read(&mut [0u8; 1]).unwrap(), 0);
    }

    #[test]
    fn test_read_buffer_exact_fit() {
        let data = b"01234567";
        let mut reader = BufferedReader::with_capacity(data.as_ref(), 8);
        let mut buf = [0u8; 8];
        let n = reader.read(&mut buf).unwrap();
        assert_eq!(n, 8);
        assert_eq!(&buf, b"01234567");
        assert_eq!(reader.position(), 8);
    }

    #[test]
    fn test_read_buffer_larger_than_data() {
        let data = b"rust";
        let mut reader = BufferedReader::with_capacity(data.as_ref(), 10);
        let mut buf = [0u8; 10];
        let n = reader.read(&mut buf).unwrap();
        assert_eq!(n, 4);
        assert_eq!(&buf[..n], b"rust");
        assert_eq!(reader.position(), 4);
        // EOF
        assert_eq!(reader.read(&mut buf).unwrap(), 0);
    }

    #[test]
    fn test_read_exact_across_multiple_buffers() {
        let data = b"abcdefghij";
        let mut reader = BufferedReader::with_capacity(data.as_ref(), 4);
        let mut buf = [0u8; 10];
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"abcdefghij");
        assert_eq!(reader.position(), 10);
    }

    #[test]
    fn test_read_exact_after_partial_read() {
        let data = b"1234567";
        let mut reader = BufferedReader::with_capacity(data.as_ref(), 4);

        let mut head = [0u8; 3];
        reader.read(&mut head).unwrap();
        assert_eq!(&head, b"123");
        assert_eq!(reader.position(), 3);

        let mut tail = [0u8; 4];
        reader.read_exact(&mut tail).unwrap();
        assert_eq!(&tail, b"4567");
        assert_eq!(reader.position(), 7);
    }

    #[test]
    fn test_multiple_read_exact_calls() {
        let data = b"ABCDEFGHIJKLMNO";
        let mut reader = BufferedReader::with_capacity(data.as_ref(), 5);

        let mut a = [0u8; 5];
        let mut b = [0u8; 5];
        let mut c = [0u8; 5];
        reader.read_exact(&mut a).unwrap();
        reader.read_exact(&mut b).unwrap();
        reader.read_exact(&mut c).unwrap();
        assert_eq!(&a, b"ABCDE");
        assert_eq!(&b, b"FGHIJ");
        assert_eq!(&c, b"KLMNO");
        assert_eq!(reader.position(), 15);
    }

    #[test]
    fn test_read_propagates_underlying_error() {
        let mut reader = BufferedReader::with_capacity(ErrReader, 4);
        let mut buf = [0u8; 3];
        let err = reader.read(&mut buf).unwrap_err();
        assert!(matches!(err, Error::InvalidData));
        assert_eq!(reader.position(), 0);
    }

    #[test]
    fn test_read_exact_propagates_underlying_error() {
        let mut reader = BufferedReader::with_capacity(ErrReader, 4);
        let mut buf = [0u8; 3];
        let err = reader.read_exact(&mut buf).unwrap_err();
        assert!(matches!(err, Error::InvalidData));
        assert_eq!(reader.position(), 0);
    }

    /// A reader that returns at most `chunk` bytes per call.
    struct ChunkReader {
        data: Vec<u8>,
        chunk: usize,
        pos: usize,
    }
    impl Read for ChunkReader {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
            if self.pos >= self.data.len() {
                return Ok(0);
            }
            let rem = &self.data[self.pos..];
            let take = rem.len().min(self.chunk).min(buf.len());
            buf[..take].copy_from_slice(&rem[..take]);
            self.pos += take;
            Ok(take)
        }
    }

    #[test]
    fn test_read_exact_from_chunk_reader() {
        let text = b"The quick brown fox jumps over the lazy dog";
        let inner = ChunkReader {
            data: text.to_vec(),
            chunk: 7,
            pos: 0,
        };
        let mut reader = BufferedReader::with_capacity(inner, 16);
        let mut out = vec![0u8; text.len()];
        // read_exact should transparently stitch together multiple small chunks
        reader.read_exact(&mut out).unwrap();
        assert_eq!(&out, text);
        assert_eq!(reader.position(), text.len());
    }
}
