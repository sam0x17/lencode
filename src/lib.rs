#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::collections;
#[cfg(all(test, not(feature = "std")))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(all(test, not(feature = "std")))]
use alloc::vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
#[cfg(feature = "std")]
use std::collections;

pub mod bit_varint;
pub mod io;
pub mod tuples;
pub mod varint;

pub mod prelude {
    pub use super::*;
    pub use crate::bit_varint::*;
    pub use crate::io::*;
    pub use crate::varint::lencode::*;
    pub use crate::varint::*;
}

use prelude::*;

pub type Result<T> = core::result::Result<T, Error>;

pub trait Encode {
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize>;

    fn encode_len<S: Scheme>(len: usize, writer: &mut impl Write) -> Result<usize> {
        S::encode_varint(len as u64, writer)
    }
}

pub trait Decode {
    fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self>
    where
        Self: Sized;

    fn decode_len<S: Scheme>(reader: &mut impl Read) -> Result<usize> {
        S::decode_varint::<u64>(reader).map(|v| v as usize)
    }
}

macro_rules! impl_encode_decode_unsigned_primitive {
    ($($t:ty),*) => {
        $(
            impl Encode for $t {
                #[inline(always)]
                fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
                    S::encode_varint(*self, writer)
                }
            }

            impl Decode for $t {
                #[inline(always)]
                fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
                    S::decode_varint(reader)
                }

                #[inline(always)]
                fn decode_len<S: Scheme>(_reader: &mut impl Read) -> Result<usize> {
                    unimplemented!()
                }
            }
        )*
    };
}

impl_encode_decode_unsigned_primitive!(u16, u32, u64, u128);

impl Encode for usize {
    #[inline(always)]
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
        S::encode_varint(*self as u64, writer)
    }
}

impl Decode for usize {
    #[inline(always)]
    fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
        S::decode_varint(reader).map(|v: u64| v as usize)
    }

    #[inline(always)]
    fn decode_len<S: Scheme>(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

macro_rules! impl_encode_decode_signed_primitive {
    ($($t:ty),*) => {
        $(
            impl Encode for $t {
                #[inline(always)]
                fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
                    S::encode_varint_signed(*self, writer)
                }
            }

            impl Decode for $t {
                #[inline(always)]
                fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
                    S::decode_varint_signed(reader)
                }

                #[inline(always)]
                fn decode_len<S: Scheme>(_reader: &mut impl Read) -> Result<usize> {
                    unimplemented!()
                }
            }
        )*
    };
}

impl_encode_decode_signed_primitive!(i16, i32, i64, i128);

impl Encode for isize {
    #[inline(always)]
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
        S::encode_varint_signed(*self as i64, writer)
    }
}

impl Decode for isize {
    #[inline(always)]
    fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
        S::decode_varint_signed(reader).map(|v: i64| v as isize)
    }

    #[inline(always)]
    fn decode_len<S: Scheme>(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl Encode for bool {
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
        S::encode_bool(*self, writer)
    }
}

impl Decode for bool {
    fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
        S::decode_bool(reader)
    }

    fn decode_len<S: Scheme>(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl Encode for &[u8] {
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len::<S>(self.len(), writer)?;
        total_written += writer.write(self)?;
        Ok(total_written)
    }
}

impl Encode for &str {
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len::<S>(self.len(), writer)?;
        total_written += writer.write(self.as_bytes())?;
        Ok(total_written)
    }
}

impl<T: Encode> Encode for Option<T> {
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
        match self {
            Some(value) => {
                let mut total_written = 0;
                total_written += S::encode_bool(true, writer)?;
                total_written += value.encode::<S>(writer)?;
                Ok(total_written)
            }
            None => S::encode_bool(false, writer),
        }
    }
}

impl<T: Decode> Decode for Option<T> {
    fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
        if S::decode_bool(reader)? {
            Ok(Some(T::decode::<S>(reader)?))
        } else {
            Ok(None)
        }
    }

    fn decode_len<S: Scheme>(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<T: Encode, E: Encode> Encode for core::result::Result<T, E> {
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
        match self {
            Ok(value) => {
                let mut total_written = 0;
                total_written += S::encode_bool(true, writer)?;
                total_written += value.encode::<S>(writer)?;
                Ok(total_written)
            }
            Err(err) => {
                let mut total_written = 0;
                total_written += S::encode_bool(false, writer)?;
                total_written += err.encode::<S>(writer)?;
                Ok(total_written)
            }
        }
    }
}

impl<T: Decode, E: Decode> Decode for core::result::Result<T, E> {
    fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
        if S::decode_bool(reader)? {
            Ok(Ok(T::decode::<S>(reader)?))
        } else {
            Ok(Err(E::decode::<S>(reader)?))
        }
    }

    fn decode_len<S: Scheme>(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<const N: usize, T: Encode + Default + Copy> Encode for [T; N] {
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        for item in self {
            total_written += item.encode::<S>(writer)?;
        }
        Ok(total_written)
    }
}

impl<const N: usize, T: Decode + Default + Copy> Decode for [T; N] {
    fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
        let mut arr = [T::default(); N];
        for item in &mut arr {
            *item = T::decode::<S>(reader)?;
        }
        Ok(arr)
    }

    fn decode_len<S: Scheme>(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<T: Decode> Decode for Vec<T> {
    fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
        let len = Self::decode_len::<S>(reader)?;
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(T::decode::<S>(reader)?);
        }
        Ok(vec)
    }
}

impl<T: Encode> Encode for Vec<T> {
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len::<S>(self.len(), writer)?;
        for item in self {
            total_written += item.encode::<S>(writer)?;
        }
        Ok(total_written)
    }
}

impl<K: Encode, V: Encode> Encode for collections::BTreeMap<K, V> {
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len::<S>(self.len(), writer)?;
        for (key, value) in self {
            total_written += key.encode::<S>(writer)?;
            total_written += value.encode::<S>(writer)?;
        }
        Ok(total_written)
    }
}

impl<K: Decode + Ord, V: Decode> Decode for collections::BTreeMap<K, V> {
    fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
        let len = Self::decode_len::<S>(reader)?;
        let mut map = collections::BTreeMap::new();
        for _ in 0..len {
            let key = K::decode::<S>(reader)?;
            let value = V::decode::<S>(reader)?;
            map.insert(key, value);
        }
        Ok(map)
    }
}

impl<V: Encode> Encode for collections::BTreeSet<V> {
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len::<S>(self.len(), writer)?;
        for value in self {
            total_written += value.encode::<S>(writer)?;
        }
        Ok(total_written)
    }
}

impl<V: Decode + Ord> Decode for collections::BTreeSet<V> {
    fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
        let len = Self::decode_len::<S>(reader)?;
        let mut set = collections::BTreeSet::new();
        for _ in 0..len {
            let value = V::decode::<S>(reader)?;
            set.insert(value);
        }
        Ok(set)
    }
}

impl<V: Encode> Encode for collections::VecDeque<V> {
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len::<S>(self.len(), writer)?;
        for value in self {
            total_written += value.encode::<S>(writer)?;
        }
        Ok(total_written)
    }
}

impl<V: Decode> Decode for collections::VecDeque<V> {
    fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
        let len = Self::decode_len::<S>(reader)?;
        let mut deque = collections::VecDeque::with_capacity(len);
        for _ in 0..len {
            let value = V::decode::<S>(reader)?;
            deque.push_back(value);
        }
        Ok(deque)
    }
}

impl<V: Encode> Encode for collections::LinkedList<V> {
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len::<S>(self.len(), writer)?;
        for value in self {
            total_written += value.encode::<S>(writer)?;
        }
        Ok(total_written)
    }
}

impl<V: Decode> Decode for collections::LinkedList<V> {
    fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
        let len = Self::decode_len::<S>(reader)?;
        let mut list = collections::LinkedList::new();
        for _ in 0..len {
            let value = V::decode::<S>(reader)?;
            list.push_back(value);
        }
        Ok(list)
    }
}

impl<T: Encode> Encode for collections::BinaryHeap<T> {
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len::<S>(self.len(), writer)?;
        for value in self {
            total_written += value.encode::<S>(writer)?;
        }
        Ok(total_written)
    }
}
impl<T: Decode + Ord> Decode for collections::BinaryHeap<T> {
    fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
        let len = Self::decode_len::<S>(reader)?;
        let mut heap = collections::BinaryHeap::with_capacity(len);
        for _ in 0..len {
            let value = T::decode::<S>(reader)?;
            heap.push(value);
        }
        Ok(heap)
    }
}

#[cfg(feature = "std")]
impl<K: Encode, V: Encode> Encode for std::collections::HashMap<K, V> {
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len::<S>(self.len(), writer)?;
        for (key, value) in self {
            total_written += key.encode::<S>(writer)?;
            total_written += value.encode::<S>(writer)?;
        }
        Ok(total_written)
    }
}

#[cfg(feature = "std")]
impl<K: Decode + Eq + std::hash::Hash, V: Decode> Decode for std::collections::HashMap<K, V> {
    fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
        let len = Self::decode_len::<S>(reader)?;
        let mut map = std::collections::HashMap::with_capacity(len);
        for _ in 0..len {
            let key = K::decode::<S>(reader)?;
            let value = V::decode::<S>(reader)?;
            map.insert(key, value);
        }
        Ok(map)
    }
}

#[cfg(feature = "std")]
impl<V: Encode> Encode for std::collections::HashSet<V> {
    fn encode<S: Scheme>(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += Self::encode_len::<S>(self.len(), writer)?;
        for value in self {
            total_written += value.encode::<S>(writer)?;
        }
        Ok(total_written)
    }
}

#[cfg(feature = "std")]
impl<V: Decode + Eq + std::hash::Hash> Decode for std::collections::HashSet<V> {
    fn decode<S: Scheme>(reader: &mut impl Read) -> Result<Self> {
        let len = Self::decode_len::<S>(reader)?;
        let mut set = std::collections::HashSet::with_capacity(len);
        for _ in 0..len {
            let value = V::decode::<S>(reader)?;
            set.insert(value);
        }
        Ok(set)
    }
}

#[test]
fn test_encode_decode_i16_all() {
    for i in i16::MIN..=i16::MAX {
        let val: i16 = i;
        let mut buf = [0u8; 3];
        let n = i16::encode::<Lencode>(&val, &mut Cursor::new(&mut buf[..])).unwrap();
        let decoded = i16::decode::<Lencode>(&mut Cursor::new(&buf[..n])).unwrap();
        assert_eq!(decoded, val);
    }
}

#[test]
fn test_encode_decode_vec_of_i16_all() {
    let values: Vec<i16> = (i16::MIN..=i16::MAX).collect();
    let mut buf = vec![0u8; 3 * values.len()];
    let n = values
        .encode::<Lencode>(&mut Cursor::new(&mut buf[..]))
        .unwrap();
    assert!(n < values.len() * 3);
    let decoded = Vec::<i16>::decode::<Lencode>(&mut Cursor::new(&buf[..n])).unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn test_encode_decode_vec_of_many_small_u128() {
    let values: Vec<u128> = (0..(u16::MAX / 2) as u128)
        .chain(0..(u16::MAX / 2) as u128)
        .collect();
    let mut buf = vec![0u8; 3 * values.len()];
    let n = values
        .encode::<Lencode>(&mut Cursor::new(&mut buf[..]))
        .unwrap();
    assert!(n < values.len() * 3);
    let decoded = Vec::<u128>::decode::<Lencode>(&mut Cursor::new(&buf[..n])).unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn test_encode_decode_vec_of_tiny_u128s() {
    let values: Vec<u128> = (0..127).collect();
    let mut buf = vec![0u8; values.len() + 1];
    let n = values
        .encode::<Lencode>(&mut Cursor::new(&mut buf[..]))
        .unwrap();
    assert_eq!(n, values.len() + 1);
    let decoded = Vec::<u128>::decode::<Lencode>(&mut Cursor::new(&buf[..n])).unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn test_encode_decode_bools() {
    let values = vec![true, false, true, false, true];
    let mut buf = vec![0u8; values.len() + 1];
    let n = values
        .encode::<Lencode>(&mut Cursor::new(&mut buf[..]))
        .unwrap();
    assert_eq!(n, values.len() + 1);
    let decoded = Vec::<bool>::decode::<Lencode>(&mut Cursor::new(&buf[..n])).unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn test_encode_decode_option() {
    let values = vec![Some(42), None, Some(100), None, Some(200)];
    let mut buf = vec![0u8; 12];
    let n = values
        .encode::<Lencode>(&mut Cursor::new(&mut buf[..]))
        .unwrap();
    assert_eq!(n, buf.len());
    let decoded = Vec::<Option<i32>>::decode::<Lencode>(&mut Cursor::new(&buf[..n])).unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn test_encode_decode_arrays() {
    let values: [u128; 5] = [1, 2, 3, 4, 5];
    let mut buf = vec![0u8; 5];
    let n = values
        .encode::<Lencode>(&mut Cursor::new(&mut buf[..]))
        .unwrap();
    assert_eq!(n, 5);
    let decoded: [u128; 5] = Decode::decode::<Lencode>(&mut Cursor::new(&buf[..])).unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn test_tree_map_encode_decode() {
    let mut map = collections::BTreeMap::new();
    map.insert(1, 4);
    map.insert(2, 5);
    map.insert(3, 6);

    let mut buf = vec![0u8; 7];
    let n = map
        .encode::<Lencode>(&mut Cursor::new(&mut buf[..]))
        .unwrap();
    assert_eq!(n, 7);

    let decoded: collections::BTreeMap<i32, i32> =
        Decode::decode::<Lencode>(&mut Cursor::new(&buf[..])).unwrap();
    assert_eq!(decoded, map);
}

#[cfg(feature = "std")]
#[test]
fn test_hash_map_encode_decode() {
    let mut map = std::collections::HashMap::new();
    map.insert(1, 4);
    map.insert(2, 5);
    map.insert(3, 6);

    let mut buf = vec![0u8; 7];
    let n = map
        .encode::<Lencode>(&mut Cursor::new(&mut buf[..]))
        .unwrap();
    assert_eq!(n, 7);

    let decoded: std::collections::HashMap<i32, i32> =
        Decode::decode::<Lencode>(&mut Cursor::new(&buf[..])).unwrap();
    assert_eq!(decoded, map);
}

#[cfg(feature = "std")]
#[test]
fn test_hash_set_encode_decode() {
    let mut set = std::collections::HashSet::new();
    set.insert(1);
    set.insert(2);
    set.insert(3);

    let mut buf = vec![0u8; 4];
    let n = set
        .encode::<Lencode>(&mut Cursor::new(&mut buf[..]))
        .unwrap();
    assert_eq!(n, 4);

    let decoded: std::collections::HashSet<i32> =
        Decode::decode::<Lencode>(&mut Cursor::new(&buf[..])).unwrap();
    assert_eq!(decoded, set);
}

#[test]
fn test_btree_set_encode_decode() {
    let mut set = collections::BTreeSet::new();
    set.insert(1);
    set.insert(2);
    set.insert(3);

    let mut buf = vec![0u8; 4];
    let n = set
        .encode::<Lencode>(&mut Cursor::new(&mut buf[..]))
        .unwrap();
    assert_eq!(n, 4);

    let decoded: collections::BTreeSet<i32> =
        Decode::decode::<Lencode>(&mut Cursor::new(&buf[..])).unwrap();
    assert_eq!(decoded, set);
}

#[test]
fn test_vec_deque_encode_decode() {
    let mut deque = collections::VecDeque::new();
    deque.push_back(1);
    deque.push_back(2);
    deque.push_back(3);

    let mut buf = vec![0u8; 4];
    let n = deque
        .encode::<Lencode>(&mut Cursor::new(&mut buf[..]))
        .unwrap();
    assert_eq!(n, 4);

    let decoded: collections::VecDeque<i32> =
        Decode::decode::<Lencode>(&mut Cursor::new(&buf[..])).unwrap();
    assert_eq!(decoded, deque);
}

#[test]
fn test_linked_list_encode_decode() {
    let mut list = collections::LinkedList::new();
    list.push_back(1);
    list.push_back(2);
    list.push_back(3);

    let mut buf = vec![0u8; 4];
    let n = list
        .encode::<Lencode>(&mut Cursor::new(&mut buf[..]))
        .unwrap();
    assert_eq!(n, 4);

    let decoded: collections::LinkedList<i32> =
        Decode::decode::<Lencode>(&mut Cursor::new(&buf[..])).unwrap();
    assert_eq!(decoded, list);
}

#[test]
fn test_binary_heap_encode_decode() {
    let mut heap = collections::BinaryHeap::new();
    heap.push(1);
    heap.push(2);
    heap.push(3);

    let mut buf = vec![0u8; 4];
    let n = heap
        .encode::<Lencode>(&mut Cursor::new(&mut buf[..]))
        .unwrap();
    assert_eq!(n, 4);

    let decoded: collections::BinaryHeap<i32> =
        Decode::decode::<Lencode>(&mut Cursor::new(&buf[..])).unwrap();
    assert_eq!(
        decoded.clone().into_sorted_vec(),
        heap.clone().into_sorted_vec()
    );
}
