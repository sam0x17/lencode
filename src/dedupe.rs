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

    /// Returns the number of unique values currently stored in the encoder.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.table.len()
    }

    /// Returns true if the encoder contains no values.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
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

#[derive(Clone, Default, PartialEq, Eq)]
pub struct DedupeDecoder {
    table: Vec<Vec<u8>>,
}

impl DedupeDecoder {
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.table.clear();
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.table.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    /// Decodes a value with deduplication.
    ///
    /// If the ID is 0, a new value is decoded and stored in the table.
    /// Otherwise, the value is retrieved from the table using the given ID.
    ///
    /// # Arguments
    ///
    /// * `reader` - The reader from which the encoded data will be read.
    ///
    /// # Returns
    ///
    /// The decoded value.
    #[inline]
    pub fn decode<T: Pack>(&mut self, reader: &mut impl Read) -> Result<T> {
        let id = Lencode::decode_varint::<usize>(reader)?;

        if id == 0 {
            // New value, decode it and store in table
            let value = T::unpack(reader)?;
            let mut buf = Vec::with_capacity(core::mem::size_of::<T>());
            value.pack(&mut buf)?;
            self.table.push(buf);
            Ok(value)
        } else {
            // Existing value, retrieve from table
            let table_index = id - 1; // IDs start at 1, but table is 0-indexed
            if table_index >= self.table.len() {
                return Err(crate::io::Error::InvalidData);
            }
            let buf = &self.table[table_index];
            let mut cursor = crate::io::Cursor::new(buf);
            T::unpack(&mut cursor)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::Cursor;

    #[test]
    fn test_dedupe_encode_decode_roundtrip() {
        let mut encoder = DedupeEncoder::new();
        let mut decoder = DedupeDecoder::new();
        let mut buffer = Vec::new();

        // Test values
        let values = [42u32, 123u32, 42u32, 456u32, 123u32, 789u32, 42u32];

        // Encode all values
        for &value in &values {
            encoder.encode(&value, &mut buffer).unwrap();
        }

        // Decode all values
        let mut cursor = Cursor::new(&buffer);
        let mut decoded_values = Vec::new();

        for _ in &values {
            let decoded: u32 = decoder.decode(&mut cursor).unwrap();
            decoded_values.push(decoded);
        }

        // Verify the decoded values match the original
        assert_eq!(values.to_vec(), decoded_values);
    }

    #[test]
    fn test_dedupe_clear() {
        let mut encoder = DedupeEncoder::new();
        let mut decoder = DedupeDecoder::new();
        let mut buffer = Vec::new();

        // Encode some values
        encoder.encode(&42u32, &mut buffer).unwrap();
        encoder.encode(&123u32, &mut buffer).unwrap();

        // Clear and encode again - should start fresh
        encoder.clear();
        decoder.clear();
        buffer.clear();

        encoder.encode(&42u32, &mut buffer).unwrap(); // Should be encoded as new (ID 0)
        encoder.encode(&42u32, &mut buffer).unwrap(); // Should be encoded as reference (ID 1)

        let mut cursor = Cursor::new(&buffer);
        let decoded1: u32 = decoder.decode(&mut cursor).unwrap();
        let decoded2: u32 = decoder.decode(&mut cursor).unwrap();

        assert_eq!(decoded1, 42u32);
        assert_eq!(decoded2, 42u32);
    }

    #[test]
    fn test_dedupe_invalid_id() {
        let mut decoder = DedupeDecoder::new();
        let mut buffer = Vec::new();

        // Manually encode an invalid ID (references non-existent entry)
        Lencode::encode_varint(5usize, &mut buffer).unwrap();

        let mut cursor = Cursor::new(&buffer);
        let result: Result<u32> = decoder.decode(&mut cursor);

        assert!(result.is_err());
        matches!(result, Err(crate::io::Error::InvalidData));
    }
}
