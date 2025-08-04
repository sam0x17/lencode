use super::{Error, Read, Write};

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

        let bytes_to_read = len - pos;
        let bytes_read = buf.len().min(bytes_to_read);

        // SAFETY: `pos + bytes_read` is guaranteed to be within `data`
        // and `buf` has at least `bytes_read` bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(
                data.as_ptr().add(pos),
                buf.as_mut_ptr(),
                bytes_read,
            );
        }

        self.position = pos + bytes_read;
        Ok(bytes_read)
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

        let bytes_to_write = len - pos;
        let bytes_written = buf.len().min(bytes_to_write);

        // SAFETY: `pos + bytes_written` is guaranteed to be within `data`
        // and `buf` has at least `bytes_written` bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(
                buf.as_ptr(),
                data.as_mut_ptr().add(pos),
                bytes_written,
            );
        }

        self.position = pos + bytes_written;

        if bytes_written < buf.len() {
            return Err(Error::WriterOutOfSpace);
        }
        Ok(bytes_written)
    }

    #[inline(always)]
    fn flush(&mut self) -> Result<(), Error> {
        // No-op for an in-memory buffer
        Ok(())
    }
}
