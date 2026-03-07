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

    /// Returns the remaining unread bytes as a slice, if the reader supports
    /// zero‑copy access. Returns `None` by default.
    #[inline(always)]
    fn buf(&self) -> Option<&[u8]> {
        None
    }

    /// Advances the read position by `n` bytes without copying data.
    /// Only valid when `buf()` returned `Some` with at least `n` bytes.
    #[inline(always)]
    fn advance(&mut self, _n: usize) {}
}

/// Minimal write abstraction used by this crate in both std and no‑std modes.
pub trait Write {
    /// Writes the entire `buf` into the underlying sink when possible and
    /// returns the number of bytes written.
    fn write(&mut self, buf: &[u8]) -> Result<usize>;
    /// Flushes any internal buffers, if applicable.
    fn flush(&mut self) -> Result<()>;

    /// Returns a mutable slice of the spare capacity available for writing,
    /// if the writer supports direct access. Returns `None` by default.
    #[inline(always)]
    fn buf_mut(&mut self) -> Option<&mut [u8]> {
        None
    }

    /// Marks `n` bytes as written after writing directly to `buf_mut()`.
    /// Only valid when `buf_mut()` returned `Some` with at least `n` bytes.
    #[inline(always)]
    fn advance_mut(&mut self, _n: usize) {}

    /// Hints that at least `additional` more bytes will be written.
    ///
    /// Writers backed by growable buffers (e.g. [`VecWriter`]) use this to
    /// pre‑allocate capacity, reducing intermediate reallocations when encoding
    /// large collections. The default is a no‑op, which is correct for
    /// fixed‑capacity writers like [`Cursor`].
    #[inline(always)]
    fn reserve(&mut self, _additional: usize) {}
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

#[cfg(feature = "std")]
extern crate alloc;

/// A fast writer wrapping a `Vec<u8>` with zero‑copy `buf_mut()`/`advance_mut()` support.
///
/// In `std` mode the blanket `impl<W: std::io::Write> Write for W` covers `Vec<u8>` but
/// cannot provide `buf_mut()`, so every varint write goes through `extend_from_slice`.
/// `VecWriter` bypasses that blanket and writes directly into spare capacity.
pub struct VecWriter(pub alloc::vec::Vec<u8>);

impl Default for VecWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl VecWriter {
    /// Creates a new empty `VecWriter`.
    #[inline(always)]
    pub const fn new() -> Self {
        Self(alloc::vec::Vec::new())
    }

    /// Creates a `VecWriter` with the given capacity.
    #[inline(always)]
    pub fn with_capacity(cap: usize) -> Self {
        Self(alloc::vec::Vec::with_capacity(cap))
    }

    /// Consumes the writer and returns the inner `Vec<u8>`.
    #[inline(always)]
    pub fn into_inner(self) -> alloc::vec::Vec<u8> {
        self.0
    }

    /// Returns a reference to the inner `Vec<u8>`.
    #[inline(always)]
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl Write for VecWriter {
    #[inline(always)]
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let len = self.0.len();
        let add = buf.len();
        if add == 0 {
            return Ok(0);
        }
        self.0.reserve(add);
        unsafe {
            let dst = self.0.as_mut_ptr().add(len);
            core::ptr::copy_nonoverlapping(buf.as_ptr(), dst, add);
            self.0.set_len(len + add);
        }
        Ok(add)
    }

    #[inline(always)]
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    #[inline(always)]
    fn buf_mut(&mut self) -> Option<&mut [u8]> {
        let len = self.0.len();
        let mut cap = self.0.capacity();
        if cap - len < 17 {
            // Amortize allocation: use doubling strategy for smooth growth
            self.0.reserve(cap.max(256));
            cap = self.0.capacity();
        }
        unsafe {
            Some(core::slice::from_raw_parts_mut(
                self.0.as_mut_ptr().add(len),
                cap - len,
            ))
        }
    }

    #[inline(always)]
    fn advance_mut(&mut self, n: usize) {
        let new_len = self.0.len() + n;
        unsafe { self.0.set_len(new_len) };
    }

    #[inline(always)]
    fn reserve(&mut self, additional: usize) {
        self.0.reserve(additional);
    }
}

#[cfg(not(feature = "std"))]
impl Write for alloc::vec::Vec<u8> {
    #[inline(always)]
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let len = self.len();
        let add = buf.len();
        if add == 0 {
            return Ok(0);
        }
        self.reserve(add);
        unsafe {
            let dst = self.as_mut_ptr().add(len);
            core::ptr::copy_nonoverlapping(buf.as_ptr(), dst, add);
            self.set_len(len + add);
        }
        Ok(add)
    }

    #[inline(always)]
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    #[inline(always)]
    fn buf_mut(&mut self) -> Option<&mut [u8]> {
        let len = self.len();
        let mut cap = self.capacity();
        if cap - len < 17 {
            self.reserve(cap.max(256));
            cap = self.capacity();
        }
        unsafe {
            Some(core::slice::from_raw_parts_mut(
                self.as_mut_ptr().add(len),
                cap - len,
            ))
        }
    }

    #[inline(always)]
    fn advance_mut(&mut self, n: usize) {
        let new_len = self.len() + n;
        unsafe { self.set_len(new_len) };
    }

    #[inline(always)]
    fn reserve(&mut self, additional: usize) {
        alloc::vec::Vec::reserve(self, additional);
    }
}

#[test]
fn test_write_vec() {
    let mut my_vec = alloc::vec::Vec::new();
    let data = b"Hello, world!";

    // Test writing
    assert_eq!(my_vec.write(data).unwrap(), data.len());
    assert_eq!(my_vec, data);

    assert_eq!(my_vec, b"Hello, world!".to_vec());
}
