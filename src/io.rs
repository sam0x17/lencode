mod buffered_reader;

pub use buffered_reader::BufferedReader;

#[derive(Debug)]
pub enum Error {
    InvalidData,
    IncorrectLength,
    EndOfData,
    #[cfg(any(feature = "std", test))]
    StdIo(std::io::Error),
    #[cfg(not(any(feature = "std", test)))]
    StdIo(StdIoShim),
}

#[cfg(not(any(feature = "std", test)))]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum StdIoShim {}

#[cfg(any(feature = "std", test))]
impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::StdIo(err)
    }
}

#[cfg(any(feature = "std", test))]
impl From<Error> for std::io::Error {
    fn from(err: Error) -> Self {
        match err {
            Error::InvalidData => {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid data")
            }
            Error::IncorrectLength => {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "Incorrect length")
            }
            #[cfg(any(feature = "std", test))]
            Error::StdIo(e) => e,
            Error::EndOfData => {
                std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "End of data")
            }
        }
    }
}

pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error>;
}

pub trait Write {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error>;
}

#[cfg(any(feature = "std", test))]
impl<R: std::io::Read> Read for R {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        self.read(buf).map_err(|e| Error::from(e))
    }
}

#[cfg(any(feature = "std", test))]
impl<W: std::io::Write> Write for W {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        self.write(buf).map_err(|e| Error::from(e))
    }
}

#[cfg(not(any(feature = "std", test)))]
extern crate alloc;
#[cfg(not(any(feature = "std", test)))]
use alloc::vec::Vec;

#[cfg(not(any(feature = "std", test)))]
impl Write for Vec<u8> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        self.extend_from_slice(buf);
        Ok(buf.len())
    }
}

#[cfg(not(any(feature = "std", test)))]
impl Read for Vec<u8> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        if self.is_empty() {
            return Err(Error::EndOfData);
        }
        let len = buf.len().min(self.len());
        buf[..len].copy_from_slice(&self[..len]);
        self.drain(..len);
        Ok(len)
    }
}

#[cfg(not(any(feature = "std", test)))]
impl Read for &[u8] {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        if self.is_empty() {
            return Err(Error::EndOfData);
        }
        let len = buf.len().min(self.len());
        buf[..len].copy_from_slice(&self[..len]);
        *self = &self[len..];
        Ok(len)
    }
}
