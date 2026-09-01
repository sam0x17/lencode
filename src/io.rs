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
    /// A caller-provided decode limit was exceeded.
    DecodeLimitExceeded,
    /// Bytes remained after an exact decode completed.
    TrailingData,
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
            Error::DecodeLimitExceeded => write!(f, "A configured decode limit was exceeded"),
            Error::TrailingData => write!(f, "Trailing data remained after decoding"),
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
            Error::DecodeLimitExceeded => {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Decode limit exceeded")
            }
            Error::TrailingData => {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Trailing data")
            }
        }
    }
}

/// Resource limits enforced by [`LimitedReader`] during untrusted decoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodeLimits {
    /// Maximum encoded bytes that may be consumed.
    pub max_input_bytes: usize,
    /// Maximum element count accepted for any one variable-length collection.
    pub max_sequence_len: usize,
    /// Maximum cumulative allocation bytes claimed by decoded values.
    pub max_total_allocation: usize,
}

impl DecodeLimits {
    /// Creates a set of decode limits.
    #[inline(always)]
    pub const fn new(
        max_input_bytes: usize,
        max_sequence_len: usize,
        max_total_allocation: usize,
    ) -> Self {
        Self {
            max_input_bytes,
            max_sequence_len,
            max_total_allocation,
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

    /// Claims allocation bytes against an optional reader-owned decode budget.
    /// Readers without a budget accept the claim.
    #[inline(always)]
    fn claim_allocation(&mut self, _bytes: usize) -> Result<()> {
        Ok(())
    }

    /// Validates a variable-length collection and claims its backing storage.
    /// Readers without a budget accept the claim.
    #[inline(always)]
    fn claim_sequence(&mut self, _len: usize, _element_size: usize) -> Result<()> {
        Ok(())
    }

    /// Validates one variable-length byte blob and charges its bytes against
    /// an optional reader-owned allocation budget. Byte blobs have a distinct
    /// per-value limit from collection element counts: a valid large account
    /// data value must not force a decoder to also accept millions of entries
    /// in an unrelated collection.
    ///
    /// Readers without a distinct blob limit retain the historical behavior
    /// by treating bytes as one-byte sequence elements.
    #[inline(always)]
    fn claim_blob(&mut self, len: usize) -> Result<()> {
        self.claim_sequence(len, 1)
    }
}

/// Fills an initialized destination buffer, tolerating legitimate short reads.
///
/// A non-empty read that makes no progress is treated as end-of-input. Custom
/// [`Read`] implementations that claim to have written beyond the supplied
/// destination are rejected before their count can be used for indexing or
/// accounting.
#[inline(always)]
pub(crate) fn read_exact_bytes(reader: &mut impl Read, dest: &mut [u8]) -> Result<()> {
    let mut offset = 0usize;
    while offset < dest.len() {
        let remaining = dest.len() - offset;
        let read = reader.read(&mut dest[offset..])?;
        if read == 0 {
            return Err(Error::ReaderOutOfData);
        }
        if read > remaining {
            return Err(Error::InvalidData);
        }
        offset += read;
    }
    Ok(())
}

/// Reader adapter that bounds input consumption, collection lengths, and
/// cumulative allocation claims without changing the encoded format.
pub struct LimitedReader<R> {
    inner: R,
    limits: DecodeLimits,
    max_blob_bytes: usize,
    remaining_input: usize,
    claimed_allocation: usize,
}

impl<R> LimitedReader<R> {
    /// Wraps `inner` with `limits`.
    #[inline(always)]
    pub const fn new(inner: R, limits: DecodeLimits) -> Self {
        Self {
            inner,
            limits,
            max_blob_bytes: limits.max_sequence_len,
            remaining_input: limits.max_input_bytes,
            claimed_allocation: 0,
        }
    }

    /// Sets the maximum size of any one decoded byte blob independently of
    /// `max_sequence_len`. The default equals `max_sequence_len`, preserving
    /// the behavior of existing callers until they opt into a wider blob cap.
    #[inline(always)]
    pub const fn with_max_blob_bytes(mut self, max_blob_bytes: usize) -> Self {
        self.max_blob_bytes = max_blob_bytes;
        self
    }

    /// Returns the wrapped reader.
    #[inline(always)]
    pub const fn inner(&self) -> &R {
        &self.inner
    }

    /// Consumes the adapter and returns the wrapped reader.
    #[inline(always)]
    pub fn into_inner(self) -> R {
        self.inner
    }

    /// Returns the number of input bytes consumed through this adapter.
    #[inline(always)]
    pub const fn consumed(&self) -> usize {
        self.limits.max_input_bytes - self.remaining_input
    }

    /// Returns cumulative allocation bytes claimed by decoders.
    #[inline(always)]
    pub const fn claimed_allocation(&self) -> usize {
        self.claimed_allocation
    }
}

impl<R: Read> Read for LimitedReader<R> {
    #[inline(always)]
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.remaining_input == 0 {
            return Err(Error::DecodeLimitExceeded);
        }
        let len = buf.len().min(self.remaining_input);
        let read = self.inner.read(&mut buf[..len])?;
        if read == 0 {
            return Err(Error::ReaderOutOfData);
        }
        if read > len {
            return Err(Error::InvalidData);
        }
        self.remaining_input -= read;
        Ok(read)
    }

    #[inline(always)]
    fn buf(&self) -> Option<&[u8]> {
        self.inner
            .buf()
            .map(|buf| &buf[..buf.len().min(self.remaining_input)])
    }

    #[inline(always)]
    fn advance(&mut self, n: usize) {
        debug_assert!(n <= self.remaining_input);
        self.remaining_input -= n;
        self.inner.advance(n);
    }

    #[inline(always)]
    fn claim_allocation(&mut self, bytes: usize) -> Result<()> {
        let claimed = self
            .claimed_allocation
            .checked_add(bytes)
            .ok_or(Error::DecodeLimitExceeded)?;
        if claimed > self.limits.max_total_allocation {
            return Err(Error::DecodeLimitExceeded);
        }
        self.inner.claim_allocation(bytes)?;
        self.claimed_allocation = claimed;
        Ok(())
    }

    #[inline(always)]
    fn claim_sequence(&mut self, len: usize, element_size: usize) -> Result<()> {
        if len > self.limits.max_sequence_len {
            return Err(Error::DecodeLimitExceeded);
        }
        let bytes = len
            .checked_mul(element_size.max(1))
            .ok_or(Error::DecodeLimitExceeded)?;
        self.claim_allocation(bytes)
    }

    #[inline(always)]
    fn claim_blob(&mut self, len: usize) -> Result<()> {
        if len > self.max_blob_bytes {
            return Err(Error::DecodeLimitExceeded);
        }
        self.claim_allocation(len)
    }
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
        std::io::Read::read(self, buf).map_err(Error::from)
    }
}

#[cfg(feature = "std")]
impl<W: std::io::Write> Write for W {
    #[inline(always)]
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        std::io::Write::write_all(self, buf).map_err(Error::from)?;
        Ok(buf.len())
    }

    #[inline(always)]
    fn flush(&mut self) -> Result<()> {
        std::io::Write::flush(self).map_err(Error::from)
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
        let cap = self.0.capacity();
        let add = buf.len();
        // Grow using a doubling strategy with a 256-byte floor. Vec's default
        // `reserve` starts from `MIN_NON_ZERO_CAP = 8` for u8 which causes
        // one realloc per doubling step on early writes. A 256-byte floor
        // matches `buf_mut`'s first-alloc size and keeps growth log(n) with
        // a better constant for sequential-write workloads.
        if cap - len < add {
            self.0.reserve(cap.max(256).max(add));
        }
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
        // Leave headroom after the first bounded field so the next field does
        // not immediately force a reallocation and copy of the encoded prefix.
        let additional = if self.0.capacity() == 0 && additional != 0 && additional <= 1024 {
            additional.next_power_of_two()
        } else {
            additional
        };
        self.0.reserve(additional);
    }
}

#[cfg(not(feature = "std"))]
impl Write for alloc::vec::Vec<u8> {
    #[inline(always)]
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let len = self.len();
        let cap = self.capacity();
        let add = buf.len();
        if cap - len < add {
            self.reserve(cap.max(256).max(add));
        }
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

#[test]
fn test_vec_writer_rounds_bounded_initial_reserve() {
    let mut writer = VecWriter::new();

    Write::reserve(&mut writer, 0);
    assert_eq!(writer.0.capacity(), 0);

    Write::reserve(&mut writer, 521);
    assert!(writer.0.capacity() >= 1024);
    assert!(writer.0.is_empty());
}

#[cfg(test)]
mod robustness_tests {
    use super::*;

    struct ChunkedReader<'a> {
        input: &'a [u8],
        position: usize,
        chunk_size: usize,
    }

    impl Read for ChunkedReader<'_> {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
            if buf.is_empty() {
                return Ok(0);
            }
            let remaining = &self.input[self.position..];
            if remaining.is_empty() {
                return Ok(0);
            }
            let len = remaining.len().min(buf.len()).min(self.chunk_size);
            buf[..len].copy_from_slice(&remaining[..len]);
            self.position += len;
            Ok(len)
        }
    }

    struct ZeroReader;

    impl Read for ZeroReader {
        fn read(&mut self, _buf: &mut [u8]) -> Result<usize> {
            Ok(0)
        }
    }

    struct OverreportReader;

    impl Read for OverreportReader {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
            Ok(buf.len().saturating_add(1))
        }
    }

    #[test]
    fn exact_read_accepts_chunks_and_rejects_invalid_progress() {
        let mut output = [0u8; 5];
        let mut chunked = ChunkedReader {
            input: b"hello",
            position: 0,
            chunk_size: 1,
        };
        read_exact_bytes(&mut chunked, &mut output).unwrap();
        assert_eq!(&output, b"hello");

        assert!(matches!(
            read_exact_bytes(&mut ZeroReader, &mut output),
            Err(Error::ReaderOutOfData)
        ));
        assert!(matches!(
            read_exact_bytes(&mut OverreportReader, &mut output),
            Err(Error::InvalidData)
        ));

        let mut truncated = ChunkedReader {
            input: b"no",
            position: 0,
            chunk_size: 1,
        };
        assert!(matches!(
            read_exact_bytes(&mut truncated, &mut output),
            Err(Error::ReaderOutOfData)
        ));
    }

    #[test]
    fn limited_reader_rejects_zero_and_overreported_reads_without_accounting_them() {
        let limits = DecodeLimits::new(8, 8, 8);
        let mut output = [0u8; 4];

        let mut zero = LimitedReader::new(ZeroReader, limits);
        assert!(matches!(
            Read::read(&mut zero, &mut output),
            Err(Error::ReaderOutOfData)
        ));
        assert_eq!(zero.consumed(), 0);

        let mut overreported = LimitedReader::new(OverreportReader, limits);
        assert!(matches!(
            Read::read(&mut overreported, &mut output),
            Err(Error::InvalidData)
        ));
        assert_eq!(overreported.consumed(), 0);
    }

    #[cfg(feature = "std")]
    #[test]
    fn std_write_adapter_retries_short_writes() {
        #[derive(Default)]
        struct ShortWriter {
            output: Vec<u8>,
            calls: usize,
        }

        impl std::io::Write for ShortWriter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                if buf.is_empty() {
                    return Ok(0);
                }
                let len = buf.len().min(2);
                self.output.extend_from_slice(&buf[..len]);
                self.calls += 1;
                Ok(len)
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let mut writer = ShortWriter::default();
        assert_eq!(Write::write(&mut writer, b"short writes").unwrap(), 12);
        assert_eq!(writer.output, b"short writes");
        assert!(writer.calls > 1);
    }
}
