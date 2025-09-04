#![cfg_attr(not(feature = "std"), no_std)]

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

pub mod dedupe;
pub mod io;
pub mod pack;
pub mod tuples;
pub mod u256;
pub mod varint;

#[cfg(feature = "solana")]
pub mod solana;

pub mod prelude {
    pub use super::*;
    pub use crate::io::*;
    pub use crate::pack::*;
    pub use crate::u256::*;
    pub use crate::varint::*;
    pub use dedupe::*;
}

use prelude::*;

// Provide a Result alias that defaults to this crate's [`Error`] type while
// still allowing callers (and macros) to specify a different error type when
// needed. This avoids clashing with macros that expect the standard `Result`
// alias to accept two generic parameters.
pub type Result<T, E = Error> = core::result::Result<T, E>;

pub trait Encode {
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize>;

    #[inline(always)]
    fn encode_len(len: usize, writer: &mut impl Write) -> Result<usize> {
        Lencode::encode_varint(len as u64, writer)
    }

    #[inline(always)]
    fn encode(&self, writer: &mut impl Write) -> Result<usize> {
        self.encode_ext(writer, None)
    }
}

pub trait Decode {
    fn decode_ext(
        reader: &mut impl Read,
        dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self>
    where
        Self: Sized;

    #[inline(always)]
    fn decode_len(reader: &mut impl Read) -> Result<usize> {
        Lencode::decode_varint::<u64>(reader).map(|v| v as usize)
    }

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
                fn encode_ext(&self, writer: &mut impl Write, _dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>) -> Result<usize> {
                    Lencode::encode_varint(*self, writer)
                }
            }

            impl Decode for $t {
                #[inline(always)]
                fn decode_ext(reader: &mut impl Read, _dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>) -> Result<Self> {
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
        _dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        Lencode::encode_varint(*self as u64, writer)
    }
}

impl Decode for usize {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        _dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
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
                fn encode_ext(&self, writer: &mut impl Write, _dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>) -> Result<usize> {
                    Lencode::encode_varint_signed(*self, writer)
                }
            }

            impl Decode for $t {
                #[inline(always)]
                fn decode_ext(reader: &mut impl Read, _dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>) -> Result<Self> {
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
        _dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        Lencode::encode_varint_signed(*self as i64, writer)
    }
}

impl Decode for isize {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        _dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
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
        _dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        Lencode::encode_bool(*self, writer)
    }
}

impl Decode for bool {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        _dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        Lencode::decode_bool(reader)
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
        _dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len(self.len(), writer)?;
        total_written += writer.write(self)?;
        Ok(total_written)
    }
}

impl Encode for &str {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        _dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len(self.len(), writer)?;
        total_written += writer.write(self.as_bytes())?;
        Ok(total_written)
    }
}

impl Encode for String {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        _dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len(self.len(), writer)?;
        total_written += writer.write(self.as_bytes())?;
        Ok(total_written)
    }
}

impl Decode for String {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        _dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        let len = Self::decode_len(reader)?;
        let mut buf = vec![0u8; len];
        reader.read(&mut buf)?;
        String::from_utf8(buf).map_err(|_| Error::InvalidData)
    }
}

impl<T: Encode> Encode for Option<T> {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
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
        dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
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
        dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
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
        dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
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
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
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
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
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

impl<T: Decode> Decode for Vec<T> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        let len = Self::decode_len(reader)?;
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(T::decode_ext(reader, dedupe_decoder.as_deref_mut())?);
        }
        Ok(vec)
    }
}

impl<T: Encode> Encode for Vec<T> {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
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
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
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
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
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
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
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
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
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

impl<V: Encode> Encode for collections::VecDeque<V> {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len(self.len(), writer)?;
        for value in self {
            total_written += value.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        }
        Ok(total_written)
    }
}

impl<V: Decode> Decode for collections::VecDeque<V> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
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
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
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
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
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
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
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
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
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
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
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
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
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
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
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
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
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
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += self
            .start
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.end.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        Ok(total_written)
    }
}

impl<T: Decode> Decode for core::ops::Range<T> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        let start = T::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let end = T::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
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
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += self
            .start()
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self
            .end()
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        Ok(total_written)
    }
}

impl<T: Decode> Decode for core::ops::RangeInclusive<T> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        let start = T::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let end = T::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
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
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        self.start.encode_ext(writer, dedupe_encoder.as_deref_mut())
    }
}

impl<T: Decode> Decode for core::ops::RangeFrom<T> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        let start = T::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
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
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        self.end.encode_ext(writer, dedupe_encoder.as_deref_mut())
    }
}

impl<T: Decode> Decode for core::ops::RangeTo<T> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        let end = T::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
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
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        self.end.encode_ext(writer, dedupe_encoder.as_deref_mut())
    }
}

impl<T: Decode> Decode for core::ops::RangeToInclusive<T> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        let end = T::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
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
        _dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        Ok(0)
    }
}

impl Decode for core::ops::RangeFull {
    #[inline(always)]
    fn decode_ext(
        _reader: &mut impl Read,
        _dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
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
        _dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        Ok(0)
    }
}

impl Decode for () {
    #[inline(always)]
    fn decode_ext(
        _reader: &mut impl Read,
        _dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
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
        _dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        Ok(0)
    }
}

impl<T: Decode> Decode for core::marker::PhantomData<T> {
    #[inline(always)]
    fn decode_ext(
        _reader: &mut impl Read,
        _dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        Ok(core::marker::PhantomData)
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
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
