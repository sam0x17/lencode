use core::hash::Hash;

use hashbrown::{HashMap, hash_map::Entry};

use crate::prelude::*;

#[derive(Clone, Default, PartialEq, Eq)]
pub struct DedupeEncoder {
    table: HashMap<Vec<u8>, usize>,
}

impl DedupeEncoder {
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.table.clear();
    }

    /// Encodes a value with deduplication.
    ///
    /// If the value has been seen before, only its ID is encoded.
    /// Otherwise, the value is encoded in full, preceded by a special ID (0).
    ///
    /// # Arguments
    ///
    /// * `val` - The value to encode. It must implement `Hash`, `Eq`, and `Encode`.
    /// * `writer` - The writer to which the encoded data will be written.
    ///
    /// # Returns
    ///
    /// The number of bytes written to the writer.
    #[inline]
    pub fn encode<T: Hash + Eq + Pack>(
        &mut self,
        val: &T,
        writer: &mut impl Write,
    ) -> Result<usize> {
        let mut buf = Vec::with_capacity(core::mem::size_of::<T>());
        val.pack(&mut buf)?;
        let len = self.table.len();
        match self.table.entry(buf) {
            Entry::Occupied(entry) => {
                // value has been seen before, encode its id
                Lencode::encode_varint(*entry.get(), writer)
            }
            Entry::Vacant(entry) => {
                // new value, assign a new ID and encode the value
                entry.insert(len + 1); // ids start at 1
                let mut total_bytes = 0;
                total_bytes += Lencode::encode_varint(0usize, writer)?; // Special ID for new values
                total_bytes += val.pack(writer)?;
                Ok(total_bytes)
            }
        }
    }
}
