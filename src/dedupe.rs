use core::hash::{BuildHasher, Hash};

use hashbrown::{DefaultHashBuilder, HashTable, hash_table::Entry};

use crate::prelude::*;

pub struct DedupeEncoder {
    table: HashTable<usize>,
}

const FIRST_OCCURRENCE_ID: usize = 0;

impl DedupeEncoder {
    #[inline(always)]
    pub const fn new() -> Self {
        DedupeEncoder {
            table: HashTable::new(),
        }
    }

    #[inline]
    pub fn encode<T: Hash + Eq + Encode>(
        &mut self,
        val: T,
        writer: &mut impl Write,
    ) -> Result<usize> {
        let hashcode = DefaultHashBuilder::default().hash_one(&val);
        let id = self.table.len() + 1; // 0 is reserved for the first occurrence of new values
        match self
            .table
            .entry(hashcode, |&stored_index| stored_index == id, |&_| hashcode)
        {
            Entry::Occupied(entry) => Lencode::encode_varint(*entry.get(), writer),
            Entry::Vacant(entry) => {
                entry.insert(id);
                Lencode::encode_varint(FIRST_OCCURRENCE_ID, writer)?;
                val.encode(writer)
            }
        }
    }
}
