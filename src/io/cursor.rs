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
        if self.position >= data.len() {
            return Err(Error::ReaderOutOfData);
        }

        let bytes_to_read = data.len() - self.position;
        let bytes_read = buf.len().min(bytes_to_read);
        buf[..bytes_read].copy_from_slice(&data[self.position..self.position + bytes_read]);
        self.position += bytes_read;

        Ok(bytes_read)
    }
}

impl<T: AsMut<[u8]>> Write for Cursor<T> {
    #[inline(always)]
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        let data = self.stream.as_mut();
        if self.position >= data.len() {
            return Err(Error::WriterOutOfSpace);
        }

        let bytes_to_write = data.len() - self.position;
        let bytes_written = buf.len().min(bytes_to_write);
        data[self.position..self.position + bytes_written].copy_from_slice(&buf[..bytes_written]);
        self.position += bytes_written;

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
