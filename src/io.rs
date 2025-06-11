mod bit_reader;
mod bit_writer;
mod cursor;

pub use bit_reader::*;
pub use bit_writer::*;
pub use cursor::*;

use crate::*;

#[allow(unused_imports)]
use bitvec::prelude::*;

#[derive(Debug)]
pub enum Error {
    InvalidData,
    IncorrectLength,
    WriterOutOfSpace,
    ReaderOutOfData,
    #[cfg(feature = "std")]
    StdIo(std::io::Error),
    #[cfg(not(feature = "std"))]
    StdIo(StdIoShim),
    #[cfg(feature = "serde")]
    Serde(String),
    #[cfg(not(feature = "serde"))]
    Serde(String),
}

#[cfg(not(feature = "std"))]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum StdIoShim {}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::InvalidData => write!(
                f,
                "Invalid data was encountered (corrupted or incorrect bits/bytes in data stream)"
            ),
            Error::IncorrectLength => write!(f, "Incorrect length"),
            Error::WriterOutOfSpace => write!(f, "Tried to write past the capacity of the writer"),
            Error::ReaderOutOfData => write!(
                f,
                "Tried to read past the end of the reader's available data"
            ),
            #[cfg(feature = "std")]
            Error::StdIo(e) => write!(f, "IO error: {}", e),
            #[cfg(not(feature = "std"))]
            Error::StdIo(_) => write!(f, "IO error (shimmed)"),
            #[cfg(feature = "serde")]
            Error::Serde(e) => write!(f, "Serde error: {}", e),
            #[cfg(not(feature = "serde"))]
            Error::Serde(_) => {
                write!(f, "Serde error (shimmed)")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}

#[cfg(feature = "std")]
impl From<std::io::Error> for Error {
    #[inline(always)]
    fn from(err: std::io::Error) -> Self {
        Error::StdIo(err)
    }
}

#[cfg(feature = "std")]
impl From<Error> for std::io::Error {
    fn from(err: Error) -> Self {
        match err {
            Error::WriterOutOfSpace => {
                std::io::Error::new(std::io::ErrorKind::WriteZero, "Write short")
            }
            Error::InvalidData => {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid data")
            }
            Error::IncorrectLength => {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "Incorrect length")
            }
            #[cfg(feature = "std")]
            Error::StdIo(e) => e,
            Error::ReaderOutOfData => {
                std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "End of data")
            }
            Error::Serde(e) => {
                std::io::Error::new(std::io::ErrorKind::Other, format!("Serde error: {}", e))
            }
        }
    }
}

pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
}

pub trait Write {
    fn write(&mut self, buf: &[u8]) -> Result<usize>;
    fn flush(&mut self) -> Result<()>;
}

#[cfg(feature = "std")]
impl<R: std::io::Read> Read for R {
    #[inline(always)]
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.read(buf).map_err(Error::from)
    }
}

#[cfg(feature = "std")]
impl<W: std::io::Write> Write for W {
    #[inline(always)]
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.write(buf).map_err(Error::from)
    }

    #[inline(always)]
    fn flush(&mut self) -> Result<()> {
        self.flush().map_err(Error::from)
    }
}

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

#[cfg(not(feature = "std"))]
impl Write for Vec<u8> {
    #[inline(always)]
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.extend_from_slice(buf);
        Ok(buf.len())
    }

    #[inline(always)]
    fn flush(&mut self) -> Result<()> {
        // No-op for Vec, as it doesn't have an underlying buffer to flush
        Ok(())
    }
}

#[cfg(not(feature = "std"))]
impl<T: BitStore, O: BitOrder> Write for BitVec<T, O> {
    #[inline(always)]
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let bits = buf
            .iter()
            .flat_map(|&byte| (0..8).map(move |i| (byte >> i) & 1 != 0));
        self.extend(bits);
        Ok(buf.len())
    }

    #[inline(always)]
    fn flush(&mut self) -> Result<()> {
        // No-op for BitVec, as it doesn't have an underlying buffer to flush
        Ok(())
    }
}

#[cfg(not(feature = "std"))]
impl<T: BitStore, O: BitOrder> Read for BitVec<T, O> {
    #[inline(always)]
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.is_empty() {
            return Err(Error::ReaderOutOfData);
        }

        let mut bytes_read = 0;
        for byte in buf.iter_mut() {
            *byte = 0;
            for bit in 0..8 {
                if let Some(bit_value) = self.get(bytes_read * 8 + bit) {
                    if *bit_value {
                        *byte |= 1 << bit;
                    }
                } else {
                    return Ok(bytes_read);
                }
            }
            bytes_read += 1;
        }
        Ok(bytes_read)
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

#[test]
fn test_write_bitvec() {
    let mut my_bitvec = BitVec::<usize>::new();
    let data = b"Hello, world!";

    // Test writing
    assert_eq!(my_bitvec.write(data).unwrap(), data.len());

    // Convert to BitVec and check contents
    let expected_bits: Vec<bool> = data
        .iter()
        .flat_map(|&byte| (0..8).map(move |i| (byte >> i) & 1 != 0))
        .collect();
    assert_eq!(my_bitvec.into_iter().collect::<Vec<_>>(), expected_bits);
}
