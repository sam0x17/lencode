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

        let buf_len = buf.len();
        if buf_len == 0 {
            return Ok(0);
        }
        let available = len - pos;

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

    #[inline(always)]
    fn buf(&self) -> Option<&[u8]> {
        let data = self.stream.as_ref();
        // SAFETY: position is always maintained <= data.len() by advance()
        // which only increments after successful bounds checks in encode/decode.
        Some(unsafe { data.get_unchecked(self.position..) })
    }

    #[inline(always)]
    fn advance(&mut self, n: usize) {
        self.position += n;
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

        let to_copy = if buf_len <= available {
            buf_len
        } else {
            available
        };

        // SAFETY: `pos + to_copy` is within `data` and `buf` has at least `to_copy` bytes.
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), data.as_mut_ptr().add(pos), to_copy);
        }
        self.position = pos + to_copy;

        if to_copy == buf_len {
            Ok(buf_len)
        } else {
            Err(Error::WriterOutOfSpace)
        }
    }

    #[inline(always)]
    fn flush(&mut self) -> Result<(), Error> {
        // No-op for an in-memory buffer
        Ok(())
    }

    #[inline(always)]
    fn buf_mut(&mut self) -> Option<&mut [u8]> {
        let pos = self.position;
        let data = self.stream.as_mut();
        // SAFETY: position is always maintained <= data.len() by advance_mut()
        Some(unsafe { data.get_unchecked_mut(pos..) })
    }

    #[inline(always)]
    fn advance_mut(&mut self, n: usize) {
        self.position += n;
    }
}
