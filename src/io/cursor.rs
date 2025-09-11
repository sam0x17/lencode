use super::{Error, Read, Write};

/// In‑memory cursor implementing [`Read`]/[`Write`]
/// over a byte slice‑like buffer.
pub struct Cursor<T> {
    stream: T,
    position: usize,
}

impl<T> Cursor<T> {
    /// Creates a new [`Cursor`] with the given stream.
    #[inline(always)]
    pub const fn new(stream: T) -> Self {
        Cursor {
            stream,
            position: 0,
        }
    }

    /// Returns the position of the cursor within the underlying stream.
    #[inline(always)]
    pub const fn position(&self) -> usize {
        self.position
    }
}

impl<T: AsRef<[u8]>> Read for Cursor<T> {
    #[inline(always)]
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        let data = self.stream.as_ref();
        let pos = self.position;
        let len = data.len();

        if pos >= len {
            return Err(Error::ReaderOutOfData);
        }

        let available = len - pos;
        let buf_len = buf.len();

        if buf_len == 1 {
            // SAFETY: `pos < len` so `data[pos]` is valid, and `buf` has length 1.
            unsafe {
                *buf.get_unchecked_mut(0) = *data.get_unchecked(pos);
            }
            self.position = pos + 1;
            return Ok(1);
        }

        let to_copy = if buf_len <= available {
            buf_len
        } else {
            available
        };

        // SAFETY: `pos + to_copy` is within `data` and `buf` has at least `to_copy` bytes.
        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr().add(pos), buf.as_mut_ptr(), to_copy);
        }

        self.position = pos + to_copy;
        Ok(to_copy)
    }
}

impl<T: AsMut<[u8]>> Write for Cursor<T> {
    #[inline(always)]
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        let data = self.stream.as_mut();
        let pos = self.position;
        let len = data.len();

        if pos >= len {
            return Err(Error::WriterOutOfSpace);
        }

        let buf_len = buf.len();
        let available = len - pos;

        if buf_len <= available {
            // SAFETY: `pos + buf_len` is within `data` and `buf` has `buf_len` bytes.
            unsafe {
                core::ptr::copy_nonoverlapping(buf.as_ptr(), data.as_mut_ptr().add(pos), buf_len);
            }
            self.position = pos + buf_len;
            Ok(buf_len)
        } else {
            // SAFETY: `pos + available` is within `data` and `buf` has `available` bytes.
            unsafe {
                core::ptr::copy_nonoverlapping(buf.as_ptr(), data.as_mut_ptr().add(pos), available);
            }
            self.position = len;
            Err(Error::WriterOutOfSpace)
        }
    }

    #[inline(always)]
    fn flush(&mut self) -> Result<(), Error> {
        // No-op for an in-memory buffer
        Ok(())
    }
}
