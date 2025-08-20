use core::hash::{BuildHasher, Hash};

use hashbrown::{DefaultHashBuilder, HashTable, hash_table::Entry};

use crate::prelude::*;

pub struct DedupeEncoder {
    table: HashTable<usize>,
}

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
        let table_len = self.table.len();
        match self.table.entry(
            hashcode,
            |&stored_index| stored_index == table_len,
            |&_| hashcode,
        ) {
            Entry::Occupied(entry) => Lencode::encode_varint(*entry.get(), writer),
            Entry::Vacant(entry) => {
                entry.insert(table_len);
                val.encode(writer)
            }
        }
    }
}
