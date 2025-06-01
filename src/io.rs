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

#[cfg(not(feature = "std"))]
impl Write for bitvec::vec::BitVec {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        let bits = buf
            .iter()
            .flat_map(|&byte| (0..8).map(move |i| (byte >> i) & 1 != 0));
        self.extend(bits);
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Error> {
        // No-op for BitVec, as it doesn't have an underlying buffer to flush
        Ok(())
    }
}

#[cfg(not(feature = "std"))]
impl Read for bitvec::vec::BitVec {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        if self.is_empty() {
            return Err(Error::EndOfData);
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
    let mut my_bitvec = bitvec::vec::BitVec::new();
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
