use super::{Error, Read};

pub struct Cursor<T: AsRef<[u8]>> {
    reader: T,
    position: usize,
}

impl<T: AsRef<[u8]>> Cursor<T> {
    pub fn new(reader: T) -> Self {
        Cursor {
            reader,
            position: 0,
        }
    }

    pub fn position(&self) -> usize {
        self.position
    }

    pub fn set_position(&mut self, pos: usize) -> Result<(), Error> {
        if pos > self.reader.as_ref().len() {
            return Err(Error::EndOfData);
        }
        self.position = pos;
        Ok(())
    }
}

impl<T: AsRef<[u8]>> Read for Cursor<T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        let data = self.reader.as_ref();
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
