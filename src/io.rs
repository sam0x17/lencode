mod bit_reader;
mod bit_writer;
mod cursor;

pub use bit_reader::*;
pub use bit_writer::*;
pub use cursor::*;

#[derive(Debug)]
pub enum Error {
    InvalidData,
    IncorrectLength,
    WriteShort,
    EndOfData,
    #[cfg(feature = "std")]
    StdIo(std::io::Error),
    #[cfg(not(feature = "std"))]
    StdIo(StdIoShim),
}

#[cfg(not(feature = "std"))]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum StdIoShim {}

#[cfg(feature = "std")]
impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::StdIo(err)
    }
}

#[cfg(feature = "std")]
impl From<Error> for std::io::Error {
    fn from(err: Error) -> Self {
        match err {
            Error::WriteShort => std::io::Error::new(std::io::ErrorKind::WriteZero, "Write short"),
            Error::InvalidData => {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid data")
            }
            Error::IncorrectLength => {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "Incorrect length")
            }
            #[cfg(feature = "std")]
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
    fn flush(&mut self) -> Result<(), Error>;
}

#[cfg(feature = "std")]
impl<R: std::io::Read> Read for R {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        self.read(buf).map_err(|e| Error::from(e))
    }
}

#[cfg(feature = "std")]
impl<W: std::io::Write> Write for W {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        self.write(buf).map_err(|e| Error::from(e))
    }

    fn flush(&mut self) -> Result<(), Error> {
        self.flush().map_err(|e| Error::from(e))
    }
}

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

#[cfg(not(feature = "std"))]
impl Write for Vec<u8> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        self.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Error> {
        // No-op for Vec, as it doesn't have an underlying buffer to flush
        Ok(())
    }
}

#[test]
fn test_write_vec() {
    let mut my_vec = Vec::new();
    let data = b"Hello, world!";

    // Test writing
    assert_eq!(my_vec.write(data).unwrap(), data.len());
    assert_eq!(my_vec, data);

    assert_eq!(my_vec, b"Hello, world!".to_vec());
}
