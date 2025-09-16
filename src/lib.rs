#![cfg_attr(not(feature = "std"), no_std)]
//! Compact, fast binary encoding with varints and optional deduplication.
//!
//! This crate provides two core traits, [`Encode`] and [`Decode`], for serializing types to a
//! [`Write`] and deserializing from a [`Read`] without relying on `std`. Integer types use a
//! compact variable‑length scheme (see [`Lencode`]) that encodes small values in a single byte
//! while remaining efficient for large values.
//!
//! Optional deduplication can be enabled per encode/decode call via
//! [`DedupeEncoder`]/[`DedupeDecoder`], which replaces repeated values with small IDs to
//! reduce size for data with many duplicates.
//!
//! Derive macros for [`Encode`] and [`Decode`] are available from the companion crate
//! [`lencode_macros`] and re‑exported in [`prelude`].
//!
//! Quick start:
//!
//! ```rust
//! use lencode::prelude::*;
//!
//! #[derive(Encode, Decode, PartialEq, Debug)]
//! struct Point { x: u64, y: u64 }
//!
//! let p = Point { x: 3, y: 5 };
//! let mut buf = Vec::new();
//! let _n = encode(&p, &mut buf).unwrap();
//! let q: Point = decode(&mut Cursor::new(&buf)).unwrap();
//! assert_eq!(p, q);
//! ```
//!
//! Collections and primitives:
//!
//! ```rust
//! use lencode::prelude::*;
//!
//! let values: Vec<u128> = (0..10).collect();
//! let mut buf = Vec::new();
//! encode(&values, &mut buf).unwrap();
//! let roundtrip: Vec<u128> = decode(&mut Cursor::new(&buf)).unwrap();
//! assert_eq!(values, roundtrip);
//! ```
//!
//! Deduplication (smaller output for repeated values):
//!
//! ```rust
//! use lencode::prelude::*;
//!
//! // A small type we want to dedupe; implements Pack and the dedupe markers.
//! // Note that this is a toy example, in practice `MyId` would be more
//! // efficiently encoded using regular lencode encoding because it wraps a u32.
//! #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
//! struct MyId(u32);
//!
//! impl Pack for MyId {
//!     fn pack(&self, w: &mut impl Write) -> Result<usize> { self.0.pack(w) }
//!     fn unpack(r: &mut impl Read) -> Result<Self> { Ok(Self(u32::unpack(r)?)) }
//! }
//! impl DedupeEncodeable for MyId {}
//! impl DedupeDecodeable for MyId {}
//!
//! // Prepare some data with many repeats
//! let vals = vec![MyId(42), MyId(7), MyId(42), MyId(7), MyId(42), MyId(7), MyId(42)];
//!
//! // Encode without deduplication
//! let mut plain = Vec::new();
//! encode(&vals, &mut plain).unwrap();
//!
//! // Encode with deduplication enabled
//! let mut enc = DedupeEncoder::new();
//! let mut deduped = Vec::new();
//! encode_ext(&vals, &mut deduped, Some(&mut enc)).unwrap();
//! assert!(deduped.len() < plain.len());
//!
//! // Round-trip decoding with a DedupeDecoder
//! let mut dec = DedupeDecoder::new();
//! let rt: Vec<MyId> = decode_ext(&mut Cursor::new(&deduped), Some(&mut dec)).unwrap();
//! assert_eq!(rt, vals);
//! ```

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::collections;
#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
#[cfg(feature = "std")]
use std::collections;

mod bytes;
pub mod dedupe;
pub mod io;
pub mod pack;
pub mod tuples;
pub mod u256;
pub mod varint;

#[cfg(feature = "solana")]
pub mod solana;

/// Convenience re‑exports for common traits, modules and derive macros.
pub mod prelude {
    pub use super::*;
    pub use crate::dedupe::*;
    pub use crate::io::*;
    pub use crate::pack::*;
    pub use crate::u256::*;
    pub use crate::varint::*;
    pub use lencode_macros::*;
}

use prelude::*;

/// Encodes `value` into `writer` using the type’s [`Encode`] implementation.
///
/// Returns the number of bytes written on success.
#[inline(always)]
pub fn encode<T: Encode>(value: &T, writer: &mut impl Write) -> Result<usize> {
    value.encode_ext(writer, None)
}

/// Decodes a value of type `T` from `reader` using `T`’s [`Decode`] implementation.
#[inline(always)]
pub fn decode<T: Decode>(reader: &mut impl Read) -> Result<T> {
    T::decode_ext(reader, None)
}

/// Encodes `value` with optional deduplication via [`DedupeEncoder`].
///
/// Pass `Some(&mut DedupeEncoder)` to enable value deduplication for supported
/// types (those that implement [`Pack`] and the dedupe marker traits). When
/// `None`, encoding proceeds normally.
#[inline(always)]
pub fn encode_ext(
    value: &impl Encode,
    writer: &mut impl Write,
    dedupe_encoder: Option<&mut DedupeEncoder>,
) -> Result<usize> {
    value.encode_ext(writer, dedupe_encoder)
}

/// Decodes a value with optional deduplication via [`DedupeDecoder`].
///
/// Pass `Some(&mut DedupeDecoder)` to enable table‑based decoding that
/// reconstructs repeated values from compact IDs. When `None`, decoding
/// proceeds normally.
#[inline(always)]
pub fn decode_ext<T: Decode>(
    reader: &mut impl Read,
    dedupe_decoder: Option<&mut DedupeDecoder>,
) -> Result<T> {
    T::decode_ext(reader, dedupe_decoder)
}

// Provide a Result alias that defaults to this crate's [`Error`] type while still allowing
// callers (and macros) to specify a different error type when needed. This avoids clashing
// with macros that expect the standard `Result` alias to accept two generic parameters.
/// Crate‑wide `Result` that defaults to [`Error`].
///
/// The second parameter remains customizable for macros that expect a two‑parameter `Result`
/// alias.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// Trait for types that can be encoded to a binary stream.
pub trait Encode {
    /// Encodes `self` to `writer`, optionally using [`DedupeEncoder`].
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize>;

    /// Encodes a collection length in a compact form.
    #[inline(always)]
    fn encode_len(len: usize, writer: &mut impl Write) -> Result<usize> {
        Lencode::encode_varint(len as u64, writer)
    }

    /// Encodes an enum discriminant in a compact, consistent form.
    ///
    /// The default uses an unsigned varint.
    #[inline(always)]
    fn encode_discriminant(discriminant: usize, writer: &mut impl Write) -> Result<usize> {
        Lencode::encode_varint(discriminant as u64, writer)
    }

    /// Convenience wrapper around [`Encode::encode_ext`] without deduplication.
    #[inline(always)]
    fn encode(&self, writer: &mut impl Write) -> Result<usize> {
        self.encode_ext(writer, None)
    }
}

/// Trait for types that can be decoded from a binary stream.
pub trait Decode {
    /// Decodes `Self` from `reader`, optionally using [`DedupeDecoder`].
    fn decode_ext(
        reader: &mut impl Read,
        dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self>
    where
        Self: Sized;

    /// Decodes a collection length previously encoded with [`Encode::encode_len`].
    #[inline(always)]
    fn decode_len(reader: &mut impl Read) -> Result<usize> {
        Lencode::decode_varint::<u64>(reader).map(|v| v as usize)
    }

    /// Decodes an enum discriminant previously encoded with [`Encode::encode_discriminant`].
    ///
    /// The default reads an unsigned varint.
    #[inline(always)]
    fn decode_discriminant(reader: &mut impl Read) -> Result<usize> {
        Lencode::decode_varint::<u64>(reader).map(|v| v as usize)
    }

    /// Convenience wrapper around [`Decode::decode_ext`] without deduplication.
    #[inline(always)]
    fn decode(reader: &mut impl Read) -> Result<Self>
    where
        Self: Sized,
    {
        Self::decode_ext(reader, None)
    }
}

macro_rules! impl_encode_decode_unsigned_primitive {
    ($($t:ty),*) => {
        $(
            impl Encode for $t {
                #[inline(always)]
                fn encode_ext(&self, writer: &mut impl Write, _dedupe_encoder: Option<&mut DedupeEncoder>) -> Result<usize> {
                    Lencode::encode_varint(*self, writer)
                }
            }

            impl Decode for $t {
                #[inline(always)]
                fn decode_ext(reader: &mut impl Read, _dedupe_decoder: Option<&mut DedupeDecoder>) -> Result<Self> {
                    Lencode::decode_varint(reader)
                }

                #[inline(always)]
                fn decode_len(_reader: &mut impl Read) -> Result<usize> {
                    unimplemented!()
                }
            }
        )*
    };
}

impl_encode_decode_unsigned_primitive!(u16, u32, u64, u128);

impl Encode for usize {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        _dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        Lencode::encode_varint(*self as u64, writer)
    }
}

impl Decode for usize {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        _dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        Lencode::decode_varint(reader).map(|v: u64| v as usize)
    }

    #[inline(always)]
    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

macro_rules! impl_encode_decode_signed_primitive {
    ($($t:ty),*) => {
        $(
            impl Encode for $t {
                #[inline(always)]
                fn encode_ext(&self, writer: &mut impl Write, _dedupe_encoder: Option<&mut DedupeEncoder>) -> Result<usize> {
                    Lencode::encode_varint_signed(*self, writer)
                }
            }

            impl Decode for $t {
                #[inline(always)]
                fn decode_ext(reader: &mut impl Read, _dedupe_decoder: Option<&mut DedupeDecoder>) -> Result<Self> {
                    Lencode::decode_varint_signed(reader)
                }

                #[inline(always)]
                fn decode_len(_reader: &mut impl Read) -> Result<usize> {
                    unimplemented!()
                }
            }
        )*
    };
}

impl_encode_decode_signed_primitive!(i16, i32, i64, i128);

impl Encode for isize {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        _dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        Lencode::encode_varint_signed(*self as i64, writer)
    }
}

impl Decode for isize {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        _dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        Lencode::decode_varint_signed(reader).map(|v: i64| v as isize)
    }

    #[inline(always)]
    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl Encode for bool {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        _dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        Lencode::encode_bool(*self, writer)
    }
}

impl Decode for bool {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        _dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        Lencode::decode_bool(reader)
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

// Floating point support for convenience in client types (e.g., UiTokenAmount)
impl Encode for f32 {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        _dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let bytes = self.to_le_bytes();
        writer.write(&bytes)
    }
}

impl Decode for f32 {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        _dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let mut buf = [0u8; 4];
        if reader.read(&mut buf)? != 4 {
            return Err(Error::ReaderOutOfData);
        }
        Ok(f32::from_le_bytes(buf))
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl Encode for f64 {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        _dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let bytes = self.to_le_bytes();
        writer.write(&bytes)
    }
}

impl Decode for f64 {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        _dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let mut buf = [0u8; 8];
        if reader.read(&mut buf)? != 8 {
            return Err(Error::ReaderOutOfData);
        }
        Ok(f64::from_le_bytes(buf))
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl Encode for &[u8] {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        _dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        // Encode as either raw or compressed with a 1-bit flag in the header:
        // header = varint((payload_len << 1) | (is_compressed as usize))
        let compressed = bytes::zstd_compress(self)?;
        let raw_len = self.len();
        let comp_len = compressed.len();
        let raw_hdr = bytes::flagged_header_len(raw_len, false);
        let comp_hdr = bytes::flagged_header_len(comp_len, true);
        if comp_len + comp_hdr < raw_len + raw_hdr {
            let mut total = 0;
            total += Self::encode_len((comp_len << 1) | 1, writer)?;
            total += writer.write(&compressed)?;
            Ok(total)
        } else {
            let mut total = 0;
            total += Self::encode_len(raw_len << 1, writer)?;
            total += writer.write(self)?;
            Ok(total)
        }
    }
}

impl Encode for &str {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        _dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        // Encode as either raw UTF-8 bytes or compressed with a 1-bit flag in header
        let bytes = self.as_bytes();
        let compressed = bytes::zstd_compress(bytes)?;
        let raw_len = bytes.len();
        let comp_len = compressed.len();
        let raw_hdr = bytes::flagged_header_len(raw_len, false);
        let comp_hdr = bytes::flagged_header_len(comp_len, true);
        if comp_len + comp_hdr < raw_len + raw_hdr {
            let mut total = 0;
            total += Self::encode_len((comp_len << 1) | 1, writer)?;
            total += writer.write(&compressed)?;
            Ok(total)
        } else {
            let mut total = 0;
            total += Self::encode_len(raw_len << 1, writer)?;
            total += writer.write(bytes)?;
            Ok(total)
        }
    }
}

impl Encode for String {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        _dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        self.as_str().encode_ext(writer, None)
    }
}

impl Decode for String {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        _dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let flagged = Self::decode_len(reader)?;
        let is_compressed = (flagged & 1) == 1;
        let payload_len = flagged >> 1;
        if is_compressed {
            let mut comp = vec![0u8; payload_len];
            let mut read = 0usize;
            while read < payload_len {
                read += reader.read(&mut comp[read..])?;
            }
            let orig_len = bytes::zstd_content_size(&comp)?;
            let out = bytes::zstd_decompress(&comp, orig_len)?;
            String::from_utf8(out).map_err(|_| Error::InvalidData)
        } else {
            let mut buf = vec![0u8; payload_len];
            let mut read = 0usize;
            while read < payload_len {
                read += reader.read(&mut buf[read..])?;
            }
            String::from_utf8(buf).map_err(|_| Error::InvalidData)
        }
    }
}

impl<T: Encode> Encode for Option<T> {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        match self {
            Some(value) => {
                let mut total_written = 0;
                total_written += Lencode::encode_bool(true, writer)?;
                total_written += value.encode_ext(writer, dedupe_encoder)?;
                Ok(total_written)
            }
            None => Lencode::encode_bool(false, writer),
        }
    }
}

impl<T: Decode> Decode for Option<T> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        if Lencode::decode_bool(reader)? {
            Ok(Some(T::decode_ext(reader, dedupe_decoder)?))
        } else {
            Ok(None)
        }
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<T: Encode, E: Encode> Encode for core::result::Result<T, E> {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        match self {
            Ok(value) => {
                let mut total_written = 0;
                total_written += Lencode::encode_bool(true, writer)?;
                total_written += value.encode_ext(writer, dedupe_encoder)?;
                Ok(total_written)
            }
            Err(err) => {
                let mut total_written = 0;
                total_written += Lencode::encode_bool(false, writer)?;
                total_written += err.encode_ext(writer, dedupe_encoder)?;
                Ok(total_written)
            }
        }
    }
}

impl<T: Decode, E: Decode> Decode for core::result::Result<T, E> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        if Lencode::decode_bool(reader)? {
            Ok(Ok(T::decode_ext(reader, dedupe_decoder)?))
        } else {
            Ok(Err(E::decode_ext(reader, dedupe_decoder)?))
        }
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<const N: usize, T: Encode + Default + Copy> Encode for [T; N] {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        for item in self {
            total_written += item.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        }
        Ok(total_written)
    }
}

impl<const N: usize, T: Decode + Default + Copy> Decode for [T; N] {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let mut arr = [T::default(); N];
        for item in &mut arr {
            *item = T::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        }
        Ok(arr)
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<T: Decode + 'static> Decode for Vec<T> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        // If T is u8, decode flagged header + payload without a leading element count.
        if core::any::TypeId::of::<T>() == core::any::TypeId::of::<u8>() {
            let flagged = Self::decode_len(reader)?;
            let is_compressed = (flagged & 1) == 1;
            let payload_len = flagged >> 1;
            if is_compressed {
                let mut comp = vec![0u8; payload_len];
                let mut read = 0usize;
                while read < payload_len {
                    read += reader.read(&mut comp[read..])?;
                }
                let orig_len = bytes::zstd_content_size(&comp)?;
                let out = bytes::zstd_decompress(&comp, orig_len)?;
                let vec_t: Vec<T> = unsafe { core::mem::transmute::<Vec<u8>, Vec<T>>(out) };
                return Ok(vec_t);
            } else {
                let mut out = vec![0u8; payload_len];
                let mut read = 0usize;
                while read < payload_len {
                    read += reader.read(&mut out[read..])?;
                }
                let vec_t: Vec<T> = unsafe { core::mem::transmute::<Vec<u8>, Vec<T>>(out) };
                return Ok(vec_t);
            }
        }

        let len = Self::decode_len(reader)?;
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(T::decode_ext(reader, dedupe_decoder.as_deref_mut())?);
        }
        Ok(vec)
    }
}

impl<T: Encode + 'static> Encode for Vec<T> {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        // If element type is u8, write as raw-or-compressed with flagged header, no element count:
        if core::any::TypeId::of::<T>() == core::any::TypeId::of::<u8>() {
            // SAFETY: when T == u8, we can view the slice as &[u8]
            let bytes: &[u8] =
                unsafe { core::slice::from_raw_parts(self.as_ptr() as *const u8, self.len()) };
            let compressed = bytes::zstd_compress(bytes)?;
            let raw_len = bytes.len();
            let comp_len = compressed.len();
            let raw_hdr = bytes::flagged_header_len(raw_len, false);
            let comp_hdr = bytes::flagged_header_len(comp_len, true);
            if comp_len + comp_hdr < raw_len + raw_hdr {
                let mut total = 0;
                total += Self::encode_len((comp_len << 1) | 1, writer)?;
                total += writer.write(&compressed)?;
                return Ok(total);
            } else {
                let mut total = 0;
                total += Self::encode_len(raw_len << 1, writer)?;
                total += writer.write(bytes)?;
                return Ok(total);
            }
        }

        let mut total_written = 0;
        total_written += Self::encode_len(self.len(), writer)?;
        for item in self {
            total_written += item.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        }
        Ok(total_written)
    }
}

impl<K: Encode, V: Encode> Encode for collections::BTreeMap<K, V> {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len(self.len(), writer)?;
        for (key, value) in self {
            total_written += key.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
            total_written += value.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        }
        Ok(total_written)
    }
}

impl<K: Decode + Ord, V: Decode> Decode for collections::BTreeMap<K, V> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let len = Self::decode_len(reader)?;
        let mut map = collections::BTreeMap::new();
        for _ in 0..len {
            let key = K::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
            let value = V::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
            map.insert(key, value);
        }
        Ok(map)
    }
}

impl<V: Encode> Encode for collections::BTreeSet<V> {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len(self.len(), writer)?;
        for value in self {
            total_written += value.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        }
        Ok(total_written)
    }
}

impl<V: Decode + Ord> Decode for collections::BTreeSet<V> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let len = Self::decode_len(reader)?;
        let mut set = collections::BTreeSet::new();
        for _ in 0..len {
            let value = V::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
            set.insert(value);
        }
        Ok(set)
    }
}

impl<V: Encode + 'static> Encode for collections::VecDeque<V> {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        if core::any::TypeId::of::<V>() == core::any::TypeId::of::<u8>() {
            // Flatten to contiguous bytes first
            let (a, b) = self.as_slices();
            // Compress concatenated bytes into a temporary buffer
            let mut tmp = Vec::with_capacity(a.len() + b.len());
            let a_u8: &[u8] =
                unsafe { core::slice::from_raw_parts(a.as_ptr() as *const u8, a.len()) };
            let b_u8: &[u8] =
                unsafe { core::slice::from_raw_parts(b.as_ptr() as *const u8, b.len()) };
            tmp.extend_from_slice(a_u8);
            tmp.extend_from_slice(b_u8);
            let compressed = bytes::zstd_compress(&tmp)?;
            let raw_len = tmp.len();
            let comp_len = compressed.len();
            let raw_hdr = bytes::flagged_header_len(raw_len, false);
            let comp_hdr = bytes::flagged_header_len(comp_len, true);
            if comp_len + comp_hdr < raw_len + raw_hdr {
                let mut total_written = 0;
                total_written += Self::encode_len((comp_len << 1) | 1, writer)?;
                total_written += writer.write(&compressed)?;
                return Ok(total_written);
            } else {
                let mut total_written = 0;
                total_written += Self::encode_len(raw_len << 1, writer)?;
                total_written += writer.write(&tmp)?;
                return Ok(total_written);
            }
        }

        let mut total_written = 0;
        total_written += Self::encode_len(self.len(), writer)?;
        for value in self {
            total_written += value.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        }
        Ok(total_written)
    }
}

impl<V: Decode + 'static> Decode for collections::VecDeque<V> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        if core::any::TypeId::of::<V>() == core::any::TypeId::of::<u8>() {
            let flagged = Self::decode_len(reader)?;
            let is_compressed = (flagged & 1) == 1;
            let payload_len = flagged >> 1;
            if is_compressed {
                let mut comp = vec![0u8; payload_len];
                let mut read = 0usize;
                while read < payload_len {
                    read += reader.read(&mut comp[read..])?;
                }
                let orig_len = bytes::zstd_content_size(&comp)?;
                let out = bytes::zstd_decompress(&comp, orig_len)?;
                // SAFETY: V == u8, so reinterpretation is sound
                let out_v: Vec<V> = unsafe { core::mem::transmute::<Vec<u8>, Vec<V>>(out) };
                let mut deque = collections::VecDeque::with_capacity(orig_len);
                deque.extend(out_v);
                return Ok(deque);
            } else {
                let mut out = vec![0u8; payload_len];
                let mut read = 0usize;
                while read < payload_len {
                    read += reader.read(&mut out[read..])?;
                }
                let out_v: Vec<V> = unsafe { core::mem::transmute::<Vec<u8>, Vec<V>>(out) };
                let mut deque = collections::VecDeque::with_capacity(payload_len);
                deque.extend(out_v);
                return Ok(deque);
            }
        }

        let len = Self::decode_len(reader)?;
        let mut deque = collections::VecDeque::with_capacity(len);
        for _ in 0..len {
            let value = V::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
            deque.push_back(value);
        }
        Ok(deque)
    }
}

impl<V: Encode> Encode for collections::LinkedList<V> {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len(self.len(), writer)?;
        for value in self {
            total_written += value.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        }
        Ok(total_written)
    }
}

impl<V: Decode> Decode for collections::LinkedList<V> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let len = Self::decode_len(reader)?;
        let mut list = collections::LinkedList::new();
        for _ in 0..len {
            let value = V::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
            list.push_back(value);
        }
        Ok(list)
    }
}

impl<T: Encode> Encode for collections::BinaryHeap<T> {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len(self.len(), writer)?;
        for value in self {
            total_written += value.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        }
        Ok(total_written)
    }
}
impl<T: Decode + Ord> Decode for collections::BinaryHeap<T> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let len = Self::decode_len(reader)?;
        let mut heap = collections::BinaryHeap::with_capacity(len);
        for _ in 0..len {
            let value = T::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
            heap.push(value);
        }
        Ok(heap)
    }
}

#[cfg(feature = "std")]
impl<K: Encode, V: Encode> Encode for std::collections::HashMap<K, V> {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len(self.len(), writer)?;
        for (key, value) in self {
            total_written += key.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
            total_written += value.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        }
        Ok(total_written)
    }
}

#[cfg(feature = "std")]
impl<K: Decode + Eq + std::hash::Hash, V: Decode> Decode for std::collections::HashMap<K, V> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let len = Self::decode_len(reader)?;
        let mut map = std::collections::HashMap::with_capacity(len);
        for _ in 0..len {
            let key = K::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
            let value = V::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
            map.insert(key, value);
        }
        Ok(map)
    }
}

#[cfg(feature = "std")]
impl<V: Encode> Encode for std::collections::HashSet<V> {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len(self.len(), writer)?;
        for value in self {
            total_written += value.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        }
        Ok(total_written)
    }
}

#[cfg(feature = "std")]
impl<V: Decode + Eq + std::hash::Hash> Decode for std::collections::HashSet<V> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let len = Self::decode_len(reader)?;
        let mut set = std::collections::HashSet::with_capacity(len);
        for _ in 0..len {
            let value = V::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
            set.insert(value);
        }
        Ok(set)
    }
}

impl<T: Encode> Encode for core::ops::Range<T> {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += self
            .start
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.end.encode_ext(writer, dedupe_encoder)?;
        Ok(total_written)
    }
}

impl<T: Decode> Decode for core::ops::Range<T> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let start = T::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let end = T::decode_ext(reader, dedupe_decoder)?;
        Ok(core::ops::Range { start, end })
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<T: Encode> Encode for core::ops::RangeInclusive<T> {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += self
            .start()
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.end().encode_ext(writer, dedupe_encoder)?;
        Ok(total_written)
    }
}

impl<T: Decode> Decode for core::ops::RangeInclusive<T> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let start = T::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let end = T::decode_ext(reader, dedupe_decoder)?;
        Ok(core::ops::RangeInclusive::new(start, end))
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<T: Encode> Encode for core::ops::RangeFrom<T> {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        self.start.encode_ext(writer, dedupe_encoder)
    }
}

impl<T: Decode> Decode for core::ops::RangeFrom<T> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let start = T::decode_ext(reader, dedupe_decoder)?;
        Ok(core::ops::RangeFrom { start })
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<T: Encode> Encode for core::ops::RangeTo<T> {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        self.end.encode_ext(writer, dedupe_encoder)
    }
}

impl<T: Decode> Decode for core::ops::RangeTo<T> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let end = T::decode_ext(reader, dedupe_decoder)?;
        Ok(core::ops::RangeTo { end })
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<T: Encode> Encode for core::ops::RangeToInclusive<T> {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        self.end.encode_ext(writer, dedupe_encoder)
    }
}

impl<T: Decode> Decode for core::ops::RangeToInclusive<T> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let end = T::decode_ext(reader, dedupe_decoder)?;
        Ok(core::ops::RangeToInclusive { end })
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl Encode for core::ops::RangeFull {
    #[inline(always)]
    fn encode_ext(
        &self,
        _writer: &mut impl Write,
        _dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        Ok(0)
    }
}

impl Decode for core::ops::RangeFull {
    #[inline(always)]
    fn decode_ext(
        _reader: &mut impl Read,
        _dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        Ok(core::ops::RangeFull {})
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl Encode for () {
    #[inline(always)]
    fn encode_ext(
        &self,
        _writer: &mut impl Write,
        _dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        Ok(0)
    }
}

impl Decode for () {
    #[inline(always)]
    fn decode_ext(
        _reader: &mut impl Read,
        _dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        Ok(())
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<T: Encode> Encode for core::marker::PhantomData<T> {
    #[inline(always)]
    fn encode_ext(
        &self,
        _writer: &mut impl Write,
        _dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        Ok(0)
    }
}

impl<T: Decode> Decode for core::marker::PhantomData<T> {
    #[inline(always)]
    fn decode_ext(
        _reader: &mut impl Read,
        _dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        Ok(core::marker::PhantomData)
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

#[cfg(feature = "std")]
impl<T: Encode + Clone> Encode for std::borrow::Cow<'_, T> {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        self.as_ref().encode_ext(writer, dedupe_encoder)
    }
}

#[cfg(feature = "std")]
impl<T: Decode + Clone> Decode for std::borrow::Cow<'_, T> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        Ok(std::borrow::Cow::Owned(T::decode_ext(
            reader,
            dedupe_decoder,
        )?))
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

#[test]
fn test_encode_decode_unit_type() {
    let val = ();
    let mut buf = [0u8; 1];
    let n = val.encode(&mut Cursor::new(&mut buf[..])).unwrap();
    assert_eq!(n, 0);
    let decoded = <()>::decode(&mut Cursor::new(&buf[..n])).unwrap();
    assert_eq!(decoded, val);
}

#[test]
fn test_encode_decode_i16_all() {
    for i in i16::MIN..=i16::MAX {
        let val: i16 = i;
        let mut buf = [0u8; 3];
        let n = val.encode(&mut Cursor::new(&mut buf[..])).unwrap();
        let decoded = i16::decode(&mut Cursor::new(&buf[..n])).unwrap();
        assert_eq!(decoded, val);
    }
}

#[test]
fn test_encode_decode_vec_of_i16_all() {
    let values: Vec<i16> = (i16::MIN..=i16::MAX).collect();
    let mut buf = vec![0u8; 3 * values.len()];
    let n = values.encode(&mut Cursor::new(&mut buf[..])).unwrap();
    assert!(n < values.len() * 3);
    let decoded = Vec::<i16>::decode(&mut Cursor::new(&buf[..n])).unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn test_encode_decode_vec_of_many_small_u128() {
    let values: Vec<u128> = (0..(u16::MAX / 2) as u128)
        .chain(0..(u16::MAX / 2) as u128)
        .collect();
    let mut buf = vec![0u8; 3 * values.len()];
    let n = values.encode(&mut Cursor::new(&mut buf[..])).unwrap();
    assert!(n < values.len() * 3);
    let decoded = Vec::<u128>::decode(&mut Cursor::new(&buf[..n])).unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn test_encode_decode_vec_of_tiny_u128s() {
    let values: Vec<u128> = (0..127).collect();
    let mut buf = vec![0u8; values.len() + 1];
    let n = values.encode(&mut Cursor::new(&mut buf[..])).unwrap();
    assert_eq!(n, values.len() + 1);
    let decoded = Vec::<u128>::decode(&mut Cursor::new(&buf[..n])).unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn test_encode_decode_bools() {
    let values = vec![true, false, true, false, true];
    let mut buf = vec![0u8; values.len() + 1];
    let n = values.encode(&mut Cursor::new(&mut buf[..])).unwrap();
    assert_eq!(n, values.len() + 1);
    let decoded = Vec::<bool>::decode(&mut Cursor::new(&buf[..n])).unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn test_encode_decode_option() {
    let values = vec![Some(42), None, Some(100), None, Some(200)];
    let mut buf = [0u8; 12];
    let n = values.encode(&mut Cursor::new(&mut buf[..])).unwrap();
    assert_eq!(n, buf.len());
    let decoded = Vec::<Option<i32>>::decode(&mut Cursor::new(&buf[..n])).unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn test_encode_decode_arrays() {
    let values: [u128; 5] = [1, 2, 3, 4, 5];
    let mut buf = [0u8; 5];
    let n = values.encode(&mut Cursor::new(&mut buf[..])).unwrap();
    assert_eq!(n, 5);
    let decoded: [u128; 5] = Decode::decode(&mut Cursor::new(&buf[..])).unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn test_tree_map_encode_decode() {
    let mut map = collections::BTreeMap::new();
    map.insert(1, 4);
    map.insert(2, 5);
    map.insert(3, 6);

    let mut buf = [0u8; 7];
    let n = map.encode(&mut Cursor::new(&mut buf[..])).unwrap();
    assert_eq!(n, 7);

    let decoded: collections::BTreeMap<i32, i32> =
        Decode::decode(&mut Cursor::new(&buf[..])).unwrap();
    assert_eq!(decoded, map);
}

#[cfg(feature = "std")]
#[test]
fn test_hash_map_encode_decode() {
    let mut map = std::collections::HashMap::new();
    map.insert(1, 4);
    map.insert(2, 5);
    map.insert(3, 6);

    let mut buf = [0u8; 7];
    let n = map.encode(&mut Cursor::new(&mut buf[..])).unwrap();
    assert_eq!(n, 7);

    let decoded: std::collections::HashMap<i32, i32> =
        Decode::decode(&mut Cursor::new(&buf[..])).unwrap();
    assert_eq!(decoded, map);
}

#[cfg(feature = "std")]
#[test]
fn test_hash_set_encode_decode() {
    let mut set = std::collections::HashSet::new();
    set.insert(1);
    set.insert(2);
    set.insert(3);

    let mut buf = [0u8; 4];
    let n = set.encode(&mut Cursor::new(&mut buf[..])).unwrap();
    assert_eq!(n, 4);

    let decoded: std::collections::HashSet<i32> =
        Decode::decode(&mut Cursor::new(&buf[..])).unwrap();
    assert_eq!(decoded, set);
}

#[test]
fn test_btree_set_encode_decode() {
    let mut set = collections::BTreeSet::new();
    set.insert(1);
    set.insert(2);
    set.insert(3);

    let mut buf = [0u8; 4];
    let n = set.encode(&mut Cursor::new(&mut buf[..])).unwrap();
    assert_eq!(n, 4);

    let decoded: collections::BTreeSet<i32> = Decode::decode(&mut Cursor::new(&buf[..])).unwrap();
    assert_eq!(decoded, set);
}

#[test]
fn test_vec_deque_encode_decode() {
    let mut deque = collections::VecDeque::new();
    deque.push_back(1);
    deque.push_back(2);
    deque.push_back(3);

    let mut buf = [0u8; 4];
    let n = deque.encode(&mut Cursor::new(&mut buf[..])).unwrap();
    assert_eq!(n, 4);

    let decoded: collections::VecDeque<i32> = Decode::decode(&mut Cursor::new(&buf[..])).unwrap();
    assert_eq!(decoded, deque);
}

#[test]
fn test_linked_list_encode_decode() {
    let mut list = collections::LinkedList::new();
    list.push_back(1);
    list.push_back(2);
    list.push_back(3);

    let mut buf = [0u8; 4];
    let n = list.encode(&mut Cursor::new(&mut buf[..])).unwrap();
    assert_eq!(n, 4);

    let decoded: collections::LinkedList<i32> = Decode::decode(&mut Cursor::new(&buf[..])).unwrap();
    assert_eq!(decoded, list);
}

#[test]
fn test_binary_heap_encode_decode() {
    let mut heap = collections::BinaryHeap::new();
    heap.push(1);
    heap.push(2);
    heap.push(3);

    let mut buf = [0u8; 4];
    let n = heap.encode(&mut Cursor::new(&mut buf[..])).unwrap();
    assert_eq!(n, 4);

    let decoded: collections::BinaryHeap<i32> = Decode::decode(&mut Cursor::new(&buf[..])).unwrap();
    assert_eq!(
        decoded.clone().into_sorted_vec(),
        heap.clone().into_sorted_vec()
    );
}

#[cfg(feature = "std")]
#[test]
fn test_string_encode_decode() {
    let value = "Hello, world!";
    let mut buf = [0u8; 14];
    let n = value.encode(&mut Cursor::new(&mut buf[..])).unwrap();
    assert_eq!(n, 14);
    let decoded: String = Decode::decode(&mut Cursor::new(&buf[..])).unwrap();
    assert_eq!(decoded, value);

    let mut buf = [0u8; 14];
    let value = "";
    let n = value.encode(&mut Cursor::new(&mut buf[..])).unwrap();
    assert_eq!(n, 1);
    let decoded: String = Decode::decode(&mut Cursor::new(&buf[..])).unwrap();
    assert_eq!(decoded, value);
}

#[test]
fn test_compressed_bytes_roundtrip_vec() {
    let data: Vec<u8> = (0..200u16).map(|i| (i % 251) as u8).collect();
    let mut buf = Vec::new();
    let n = data.encode(&mut buf).unwrap();
    assert!(n > 0);
    let rt: Vec<u8> = Decode::decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(rt, data);
}

#[test]
fn test_compressed_bytes_roundtrip_slice() {
    let data: Vec<u8> = (0..200u16).map(|i| (i % 251) as u8).collect();
    let mut buf = Vec::new();
    let n = (&data[..]).encode(&mut buf).unwrap();
    assert!(n > 0);
    let rt: Vec<u8> = Decode::decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(rt, data);
}

#[test]
fn test_string_flag_raw_small_ascii() {
    use crate::prelude::*;
    let s = "Hello, Lencode!";
    let mut buf = Vec::new();
    s.encode(&mut buf).unwrap();

    // Parse flagged header
    let mut c = Cursor::new(&buf);
    let flagged = Lencode::decode_varint::<u64>(&mut c).unwrap() as usize;
    let flag = flagged & 1;
    let payload_len = flagged >> 1;
    assert_eq!(flag, 0, "expected raw path for small ASCII string");
    assert_eq!(payload_len, s.len());

    // Verify raw payload equals original bytes
    let mut header = Vec::new();
    Lencode::encode_varint(flagged as u64, &mut header).unwrap();
    assert_eq!(&buf[header.len()..], s.as_bytes());

    // Round-trip decode
    let rt: String = Decode::decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(rt, s);
}

#[test]
fn test_string_flag_compressed_repetitive_ascii() {
    use crate::prelude::*;
    // Highly compressible ASCII
    let s = core::iter::repeat('A').take(32 * 1024).collect::<String>();
    let mut buf = Vec::new();
    s.encode(&mut buf).unwrap();

    // Parse flagged header
    let mut c = Cursor::new(&buf);
    let flagged = Lencode::decode_varint::<u64>(&mut c).unwrap() as usize;
    let flag = flagged & 1;
    let payload_len = flagged >> 1;
    assert_eq!(flag, 1, "expected compressed path for repetitive string");

    // Payload length matches buffer remainder
    let mut header = Vec::new();
    Lencode::encode_varint(flagged as u64, &mut header).unwrap();
    assert_eq!(buf.len() - header.len(), payload_len);

    // Verify decompression restores original
    let payload = &buf[header.len()..];
    let frame_len = crate::bytes::zstd_content_size(payload).unwrap();
    assert_eq!(frame_len, s.len());
    let manual = crate::bytes::zstd_decompress(payload, frame_len).unwrap();
    assert_eq!(manual, s.as_bytes());

    // Round-trip decode
    let rt: String = Decode::decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(rt, s);
}

#[test]
fn test_string_flag_compressed_unicode() {
    use crate::prelude::*;
    // Compressible Unicode string (multi-byte UTF-8)
    let s = core::iter::repeat("😀").take(8192).collect::<String>();
    let mut buf = Vec::new();
    s.encode(&mut buf).unwrap();

    // Parse header and ensure compressed
    let mut c = Cursor::new(&buf);
    let flagged = Lencode::decode_varint::<u64>(&mut c).unwrap() as usize;
    assert_eq!(flagged & 1, 1);

    // Round-trip decode
    let rt: String = Decode::decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(rt, s);
}

#[test]
fn test_string_flag_corrupted_compressed_payload_errors() {
    use crate::prelude::*;
    // Ensure compression path is used
    let s = core::iter::repeat('X').take(4096).collect::<String>();
    let mut buf = Vec::new();
    s.encode(&mut buf).unwrap();

    // Get header length
    let mut c = Cursor::new(&buf);
    let flagged = Lencode::decode_varint::<u64>(&mut c).unwrap() as usize;
    assert_eq!(flagged & 1, 1);
    let mut header = Vec::new();
    Lencode::encode_varint(flagged as u64, &mut header).unwrap();

    if buf.len() > header.len() + 10 {
        let mut corrupted = buf.clone();
        corrupted[header.len() + 10] ^= 0xFF;
        let res: Result<String> = Decode::decode(&mut Cursor::new(&corrupted));
        assert!(res.is_err());
    }
}

#[test]
fn test_bytes_flag_raw_for_small_incompressible_slice() {
    use crate::prelude::*;
    // Pattern unlikely to compress
    let data: Vec<u8> = (0u16..64).map(|i| (i as u8).wrapping_mul(13)).collect();
    let mut buf = Vec::new();
    (&data[..]).encode(&mut buf).unwrap();

    // Parse flagged header
    let mut c = Cursor::new(&buf);
    let flagged = Lencode::decode_varint::<u64>(&mut c).unwrap() as usize;
    let flag = flagged & 1;
    let payload_len = flagged >> 1;
    assert_eq!(flag, 0, "expected raw path for small incompressible slice");
    assert_eq!(payload_len, data.len());

    // Ensure payload bytes equal the original raw data
    let mut header = Vec::new();
    Lencode::encode_varint(flagged as u64, &mut header).unwrap();
    assert_eq!(&buf[header.len()..], &data[..]);

    // Full round-trip via Vec<u8>
    let rt: Vec<u8> = Decode::decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(rt, data);
}

#[test]
fn test_bytes_flag_compressed_for_repetitive_slice() {
    use crate::prelude::*;
    let data: Vec<u8> = vec![7; 4096];
    let mut buf = Vec::new();
    (&data[..]).encode(&mut buf).unwrap();

    // Parse flagged header
    let mut c = Cursor::new(&buf);
    let flagged = Lencode::decode_varint::<u64>(&mut c).unwrap() as usize;
    let flag = flagged & 1;
    let payload_len = flagged >> 1;
    assert_eq!(flag, 1, "expected compressed path for repetitive slice");

    // Header should be minimal; check the remainder length matches payload_len
    let mut header = Vec::new();
    Lencode::encode_varint(flagged as u64, &mut header).unwrap();
    assert_eq!(buf.len() - header.len(), payload_len);

    // Decompress payload manually and verify it matches
    let payload = &buf[header.len()..];
    let frame_len = crate::bytes::zstd_content_size(payload).unwrap();
    assert_eq!(frame_len, data.len());
    let manual = crate::bytes::zstd_decompress(payload, frame_len).unwrap();
    assert_eq!(manual, data);

    // Full round-trip via Vec<u8>
    let rt: Vec<u8> = Decode::decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(rt, data);
}

#[test]
fn test_vec_u8_flag_paths() {
    use crate::prelude::*;
    // Raw path
    let raw: Vec<u8> = (0..80).collect();
    let mut buf = Vec::new();
    raw.encode(&mut buf).unwrap();
    let mut c = Cursor::new(&buf);
    let flagged = Lencode::decode_varint::<u64>(&mut c).unwrap() as usize;
    assert_eq!(flagged & 1, 0);
    let len = flagged >> 1;
    assert_eq!(len, raw.len());
    let mut header = Vec::new();
    Lencode::encode_varint(flagged as u64, &mut header).unwrap();
    assert_eq!(&buf[header.len()..], &raw[..]);
    let rt: Vec<u8> = Decode::decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(rt, raw);

    // Compressed path
    let comp: Vec<u8> = vec![0xAB; 8192];
    let mut buf2 = Vec::new();
    comp.encode(&mut buf2).unwrap();
    let mut c2 = Cursor::new(&buf2);
    let flagged2 = Lencode::decode_varint::<u64>(&mut c2).unwrap() as usize;
    assert_eq!(flagged2 & 1, 1);
    let payload_len = flagged2 >> 1;
    let mut header2 = Vec::new();
    Lencode::encode_varint(flagged2 as u64, &mut header2).unwrap();
    assert_eq!(buf2.len() - header2.len(), payload_len);
    let payload = &buf2[header2.len()..];
    let frame_len = crate::bytes::zstd_content_size(payload).unwrap();
    assert_eq!(frame_len, comp.len());
    let manual = crate::bytes::zstd_decompress(payload, frame_len).unwrap();
    assert_eq!(manual, comp);
    let rt2: Vec<u8> = Decode::decode(&mut Cursor::new(&buf2)).unwrap();
    assert_eq!(rt2, comp);
}

#[test]
fn test_vecdeque_u8_flag_paths_roundtrip() {
    use crate::prelude::*;
    use core::iter::FromIterator;
    // Raw path likely
    let raw_vec: Vec<u8> = (0..90).map(|i| (i as u8).wrapping_mul(37)).collect();
    let raw = collections::VecDeque::from_iter(raw_vec.clone());
    let mut buf = Vec::new();
    raw.encode(&mut buf).unwrap();
    let mut c = Cursor::new(&buf);
    let flagged = Lencode::decode_varint::<u64>(&mut c).unwrap() as usize;
    assert_eq!(flagged & 1, 0);
    let len = flagged >> 1;
    assert_eq!(len, raw_vec.len());
    let mut header = Vec::new();
    Lencode::encode_varint(flagged as u64, &mut header).unwrap();
    assert_eq!(&buf[header.len()..], &raw_vec[..]);
    let rt: collections::VecDeque<u8> = Decode::decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(rt, raw);

    // Compressed path
    let comp_vec: Vec<u8> = vec![0; 10_000];
    let comp = collections::VecDeque::from_iter(comp_vec.clone());
    let mut buf2 = Vec::new();
    comp.encode(&mut buf2).unwrap();
    let mut c2 = Cursor::new(&buf2);
    let flagged2 = Lencode::decode_varint::<u64>(&mut c2).unwrap() as usize;
    assert_eq!(flagged2 & 1, 1);
    let payload_len = flagged2 >> 1;
    let mut header2 = Vec::new();
    Lencode::encode_varint(flagged2 as u64, &mut header2).unwrap();
    assert_eq!(buf2.len() - header2.len(), payload_len);
    let payload = &buf2[header2.len()..];
    let frame_len = crate::bytes::zstd_content_size(payload).unwrap();
    assert_eq!(frame_len, comp_vec.len());
    let manual = crate::bytes::zstd_decompress(payload, frame_len).unwrap();
    assert_eq!(manual, comp_vec);
    let rt2: collections::VecDeque<u8> = Decode::decode(&mut Cursor::new(&buf2)).unwrap();
    assert_eq!(rt2, comp);
}

#[test]
fn test_bytes_flag_corrupted_compressed_payload_errors() {
    use crate::prelude::*;
    // Ensure we get a compressed path
    let data: Vec<u8> = vec![1; 2048];
    let mut buf = Vec::new();
    (&data[..]).encode(&mut buf).unwrap();
    let mut c = Cursor::new(&buf);
    let flagged = Lencode::decode_varint::<u64>(&mut c).unwrap() as usize;
    assert_eq!(flagged & 1, 1);
    let mut header = Vec::new();
    Lencode::encode_varint(flagged as u64, &mut header).unwrap();
    // Corrupt a byte in the payload (if present)
    if buf.len() > header.len() {
        let idx = header.len() + core::cmp::min(10, buf.len() - header.len() - 1);
        // Flip some bits
        let mut corrupted = buf.clone();
        corrupted[idx] ^= 0xFF;
        // Decoding should fail
        let res: Result<Vec<u8>> = Decode::decode(&mut Cursor::new(&corrupted));
        assert!(res.is_err());
    }
}
