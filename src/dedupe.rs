use core::any::{Any, TypeId};
use core::hash::Hash;
use hashbrown::HashMap;

#[cfg(not(feature = "std"))]
use alloc::boxed::Box;
#[cfg(feature = "std")]
use std::boxed::Box;

use crate::prelude::*;

pub struct DedupeEncoder {
    // Store type-specific hashmaps: TypeId -> HashMap<T, usize>
    type_stores: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
    // Store values by their assigned ID for decoder compatibility
    values_by_id: HashMap<usize, Box<dyn Any + Send + Sync>>,
    // Next ID to assign (starts at 1)
    next_id: usize,
}

impl Default for DedupeEncoder {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

impl DedupeEncoder {
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            type_stores: HashMap::new(),
            values_by_id: HashMap::new(),
            next_id: 1, // Start at 1 to match decoder
        }
    }

    /// Creates a new `DedupeEncoder` with the specified capacity.
    ///
    /// The encoder will be able to hold at least `capacity` unique values
    /// without reallocating.
    #[inline(always)]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            type_stores: HashMap::with_capacity(capacity),
            values_by_id: HashMap::with_capacity(capacity),
            next_id: 1,
        }
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.type_stores.clear();
        self.values_by_id.clear();
        self.next_id = 1;
    }

    /// Returns the number of unique values currently stored in the encoder.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.next_id - 1
    }

    /// Returns true if the encoder contains no values.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.next_id == 1
    }

    /// Encodes a value with deduplication.
    ///
    /// If the value has been seen before, only its ID is encoded.
    /// Otherwise, the value is encoded in full, preceded by a special ID (0).
    ///
    /// # Arguments
    ///
    /// * `val` - The value to encode. It must implement `Hash`, `Eq`, and `Pack`.
    /// * `writer` - The writer to which the encoded data will be written.
    ///
    /// # Returns
    ///
    /// The number of bytes written to the writer.
    #[inline]
    pub fn encode<T: Hash + Eq + Pack + Clone + Send + Sync + 'static>(
        &mut self,
        val: &T,
        writer: &mut impl Write,
    ) -> Result<usize> {
        let type_id = TypeId::of::<T>();

        // Get or create the type-specific store for this type
        let store = self
            .type_stores
            .entry(type_id)
            .or_insert_with(|| Box::new(HashMap::<T, usize>::new()));

        // Downcast to the concrete type
        let typed_store = store
            .downcast_mut::<HashMap<T, usize>>()
            .expect("Type mismatch in type store");

        // Check if we've already seen this value
        if let Some(&existing_id) = typed_store.get(val) {
            // Value has been seen before, encode its ID
            return Lencode::encode_varint(existing_id, writer);
        }

        // New value - assign an ID and store it
        let new_id = self.next_id;
        self.next_id += 1;

        // Store in both maps
        typed_store.insert(val.clone(), new_id);
        self.values_by_id.insert(new_id, Box::new(val.clone()));

        // Encode as new value (ID 0 followed by the actual value)
        let mut total_bytes = 0;
        total_bytes += Lencode::encode_varint(0usize, writer)?; // Special ID for new values
        total_bytes += val.pack(writer)?;
        Ok(total_bytes)
    }
}

#[derive(Default)]
pub struct DedupeDecoder {
    // Store values by their assigned ID (global across all types)
    values_by_id: HashMap<usize, Box<dyn Any + Send + Sync>>,
    // Next ID to assign (starts at 1 to match encoder)
    next_id: usize,
}

impl DedupeDecoder {
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            values_by_id: HashMap::new(),
            next_id: 1, // Start at 1 to match encoder
        }
    }

    /// Creates a new `DedupeDecoder` with the specified capacity.
    ///
    /// The decoder will be able to hold at least `capacity` cached values
    /// without reallocating.
    #[inline(always)]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            values_by_id: HashMap::with_capacity(capacity),
            next_id: 1,
        }
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.values_by_id.clear();
        self.next_id = 1;
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.next_id - 1
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.next_id == 1
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
    pub fn decode<T: Pack + Clone + Hash + Eq + Send + Sync + 'static>(
        &mut self,
        reader: &mut impl Read,
    ) -> Result<T> {
        let id = Lencode::decode_varint::<usize>(reader)?;

        if id == 0 {
            // New value, decode it and store in table
            let value = T::unpack(reader)?;

            // Store the value by its assigned ID
            let assigned_id = self.next_id;
            self.values_by_id
                .insert(assigned_id, Box::new(value.clone()));
            self.next_id += 1;

            Ok(value)
        } else {
            // Existing value, retrieve from table
            if let Some(boxed_value) = self.values_by_id.get(&id) {
                if let Some(typed_value) = boxed_value.downcast_ref::<T>() {
                    return Ok(typed_value.clone());
                }
            }

            Err(crate::io::Error::InvalidData)
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
