//! Lightweight, no-std compatible I/O traits and adapters used by the [`Encode`]/[`Decode`] APIs.
mod cursor;

pub use cursor::*;

use crate::*;

#[derive(Debug)]
/// Error type returned by encoding/decoding and I/O adapters.
pub enum Error {
    /// Input data was malformed or inconsistent.
    InvalidData,
    /// A size or length field was invalid for the operation.
    IncorrectLength,
    /// The writer had insufficient capacity to accept all bytes.
    WriterOutOfSpace,
    /// The reader ran out of data before the operation completed.
    ReaderOutOfData,
    #[cfg(feature = "std")]
    /// Wrapped `std::io::Error` when using the `std` feature.
    StdIo(std::io::Error),
    #[cfg(not(feature = "std"))]
    /// Placeholder for `std::io::Error` when `std` is unavailable.
    StdIo(StdIoShim),
}

#[cfg(not(feature = "std"))]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
/// Empty stand‑in used as a no‑std substitute for `std::io::Error`.
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
            Error::StdIo(e) => write!(f, "IO error: {e}"),
            #[cfg(not(feature = "std"))]
            Error::StdIo(_) => write!(f, "IO error (shimmed)"),
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
        }
    }
}

/// Minimal read abstraction used by this crate in both std and no‑std modes.
pub trait Read {
    /// Fills `buf` with bytes from the underlying source, returning the number
    /// of bytes read or an error if no data is available.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
}

/// Minimal write abstraction used by this crate in both std and no‑std modes.
pub trait Write {
    /// Writes the entire `buf` into the underlying sink when possible and
    /// returns the number of bytes written.
    fn write(&mut self, buf: &[u8]) -> Result<usize>;
    /// Flushes any internal buffers, if applicable.
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

#[test]
fn test_write_vec() {
    let mut my_vec = Vec::new();
    let data = b"Hello, world!";

    // Test writing
    assert_eq!(my_vec.write(data).unwrap(), data.len());
    assert_eq!(my_vec, data);

    assert_eq!(my_vec, b"Hello, world!".to_vec());
}
