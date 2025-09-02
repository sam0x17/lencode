use core::hash::{BuildHasher, Hash, Hasher};
use core::ops::Range;

use ahash::RandomState;
use hashbrown::HashTable;

use crate::prelude::*;

#[derive(Clone)]
pub struct DedupeEncoder {
    table: HashTable<(usize, Range<usize>)>, // (id, range into key_data)
    key_data: Vec<u8>,                       // Contiguous storage for all keys
    buffer: Vec<u8>,                         // Reusable buffer to avoid allocations
    hasher: RandomState,
}

impl Default for DedupeEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl DedupeEncoder {
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            table: HashTable::new(),
            key_data: Vec::new(),
            buffer: Vec::new(),
            hasher: RandomState::new(),
        }
    }

    /// Creates a new `DedupeEncoder` with the specified capacity.
    ///
    /// The encoder will be able to hold at least `capacity` unique values
    /// without reallocating.
    #[inline(always)]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            table: HashTable::with_capacity(capacity),
            key_data: Vec::with_capacity(capacity * 32),
            buffer: Vec::with_capacity(capacity * 32),
            hasher: RandomState::new(),
        }
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.table.clear();
        self.key_data.clear();
        self.buffer.clear();
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
        // Clear and reuse the internal buffer to avoid allocation
        self.buffer.clear();
        val.pack(&mut self.buffer)?;

        // Calculate hash for the key
        let mut hasher = self.hasher.build_hasher();
        self.buffer.hash(&mut hasher);
        let hash = hasher.finish();

        // Look for existing entry
        let found_entry = self.table.find(hash, |&(_, ref range)| {
            &self.key_data[range.clone()] == self.buffer.as_slice()
        });

        if let Some(&(id, _)) = found_entry {
            // Value has been seen before, encode its id
            Lencode::encode_varint(id, writer)
        } else {
            // New value - store it and encode
            let new_id = self.table.len() + 1; // ids start at 1

            // Store the key in contiguous memory
            let start = self.key_data.len();
            self.key_data.extend_from_slice(&self.buffer);
            let end = self.key_data.len();
            let range = start..end;

            // Insert into hash table
            self.table
                .insert_unique(hash, (new_id, range), |&(_, ref range)| {
                    let mut hasher = self.hasher.build_hasher();
                    self.key_data[range.clone()].hash(&mut hasher);
                    hasher.finish()
                });

            let mut total_bytes = 0;
            total_bytes += Lencode::encode_varint(0usize, writer)?; // Special ID for new values
            total_bytes += val.pack(writer)?;
            Ok(total_bytes)
        }
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct DedupeDecoder {
    // Single buffer to store all cached values
    data: Vec<u8>,
    // Offsets into the data buffer for each cached value (start, length)
    offsets: Vec<(usize, usize)>,
}

impl DedupeDecoder {
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new `DedupeDecoder` with the specified capacity.
    ///
    /// The decoder will be able to hold at least `capacity` cached values
    /// without reallocating.
    #[inline(always)]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity * 32),
            offsets: Vec::with_capacity(capacity),
        }
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.data.clear();
        self.offsets.clear();
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
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

            // Store the data and record its offset
            let start = self.data.len();
            let length = buf.len();
            self.data.extend_from_slice(&buf);
            self.offsets.push((start, length));

            Ok(value)
        } else {
            // Existing value, retrieve from table
            let table_index = id - 1; // IDs start at 1, but table is 0-indexed
            if table_index >= self.offsets.len() {
                return Err(crate::io::Error::InvalidData);
            }
            let (start, length) = self.offsets[table_index];
            let buf = &self.data[start..start + length];
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
