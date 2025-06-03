use super::{Error, Read, Write};

pub struct Cursor<T> {
    stream: T,
    position: usize,
}

impl<T> Cursor<T> {
    pub fn new(stream: T) -> Self {
        Cursor {
            stream,
            position: 0,
        }
    }

    pub fn position(&self) -> usize {
        self.position
    }
}

impl<T: AsRef<[u8]>> Read for Cursor<T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        let data = self.stream.as_ref();
        if self.position >= data.len() {
            return Err(Error::EndOfData);
        }

        let bytes_to_read = data.len() - self.position;
        let bytes_read = buf.len().min(bytes_to_read);
        buf[..bytes_read].copy_from_slice(&data[self.position..self.position + bytes_read]);
        self.position += bytes_read;

        Ok(bytes_read)
    }
}

impl<T: AsMut<[u8]>> Write for Cursor<T> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        let data = self.stream.as_mut();
        if self.position >= data.len() {
            return Err(Error::WriteShort);
        }

        let bytes_to_write = data.len() - self.position;
        let bytes_written = buf.len().min(bytes_to_write);
        data[self.position..self.position + bytes_written].copy_from_slice(&buf[..bytes_written]);
        self.position += bytes_written;

        if bytes_written < buf.len() {
            return Err(Error::WriteShort);
        }
        Ok(bytes_written)
    }

    fn flush(&mut self) -> Result<(), Error> {
        // No-op for an in-memory buffer
        Ok(())
    }
}
