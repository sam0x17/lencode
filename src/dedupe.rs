use core::hash::{BuildHasher, Hash};

use hashbrown::{DefaultHashBuilder, HashTable, hash_table::Entry};

use crate::prelude::*;

const FIRST_OCCURRENCE_ID: usize = 0;

#[cfg(feature = "std")]
thread_local! {
    static TABLE: std::cell::RefCell<HashTable<usize>> = std::cell::RefCell::new(HashTable::new());
}
#[cfg(not(feature = "std"))]
static TABLE: critical_section::Mutex<core::cell::RefCell<HashTable<usize>>> =
    critical_section::Mutex::new(core::cell::RefCell::new(HashTable::new()));

#[inline]
#[cfg(feature = "std")]
pub fn encode_dedupe<T: Hash + Eq + Encode>(val: T, writer: &mut impl Write) -> Result<usize> {
    let hashcode = DefaultHashBuilder::default().hash_one(&val);
    TABLE.with_borrow_mut(|table| {
        let id = table.len() + 1; // 0 is reserved for the first occurrence of new values
        match table.entry(hashcode, |&stored_index| stored_index == id, |&_| hashcode) {
            Entry::Occupied(entry) => Lencode::encode_varint(*entry.get(), writer),
            Entry::Vacant(entry) => {
                entry.insert(id);
                Lencode::encode_varint(FIRST_OCCURRENCE_ID, writer)?;
                val.encode(writer)
            }
        }
    })
}

#[inline]
#[cfg(not(feature = "std"))]
pub fn encode_dedupe<T: Hash + Eq + Encode>(val: T, writer: &mut impl Write) -> Result<usize> {
    let hashcode = DefaultHashBuilder::default().hash_one(&val);
    critical_section::with(|cs| {
        let table = TABLE.borrow(cs);
        let mut table = table.borrow_mut();
        let id = table.len() + 1; // 0 is reserved for the first occurrence of new values
        match table.entry(hashcode, |&stored_index| stored_index == id, |&_| hashcode) {
            Entry::Occupied(entry) => Lencode::encode_varint(*entry.get(), writer),
            Entry::Vacant(entry) => {
                entry.insert(id);
                Lencode::encode_varint(FIRST_OCCURRENCE_ID, writer)?;
                val.encode(writer)
            }
        }
    })
}

// TODO: actually compare values with Eq
