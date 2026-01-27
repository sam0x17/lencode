use core::any::{Any, TypeId};
use core::hash::Hash;
use hashbrown::HashMap;
use smallbox::space::S16;
use smallbox::SmallBox;

#[cfg(not(feature = "std"))]
use alloc::boxed::Box;
#[cfg(feature = "std")]
use std::boxed::Box;

use crate::prelude::*;

const DEFAULT_INITIAL_CAPACITY: usize = 128;
const DEFAULT_NUM_TYPES: usize = 4;

/// Marker trait for types eligible for deduplicated encoding.
///
/// Types must be hashable, equatable, clonable and packable so they can be
/// stored in the encoder’s table and written on first occurrence.
/// Implement this with a blanket `impl` for your type when you want
/// [`Encode::encode_ext`] to take advantage of [`DedupeEncoder`].
pub trait DedupeEncodeable: Hash + Eq + Pack + Clone + Send + Sync + 'static {}

impl<T: DedupeEncodeable> Encode for T {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        if let Some(encoder) = dedupe_encoder {
            encoder.encode(self, writer)
        } else {
            self.pack(writer)
        }
    }
}

/// Marker trait for types eligible for deduplicated decoding.
///
/// Pairs with `DedupeEncodeable`; see that trait for details.
pub trait DedupeDecodeable: Pack + Clone + Hash + Eq + Send + Sync + 'static {}

impl<T: DedupeDecodeable> Decode for T {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        if let Some(decoder) = dedupe_decoder {
            decoder.decode(reader)
        } else {
            T::unpack(reader)
        }
    }
}

/// Stateful encoder that replaces repeated values with compact IDs.
pub struct DedupeEncoder {
    // Store type-specific hashmaps: TypeId -> HashMap<T, usize>
    type_stores: HashMap<TypeId, SmallBox<dyn Any + Send + Sync, S16>>,
    // Next ID to assign (starts at 1)
    next_id: usize,
    initial_capacity: usize,
}

impl Default for DedupeEncoder {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

impl DedupeEncoder {
    /// Creates a new empty `DedupeEncoder`.
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            type_stores: HashMap::with_capacity(DEFAULT_NUM_TYPES),
            next_id: 1, // Start at 1 to match decoder
            initial_capacity: DEFAULT_INITIAL_CAPACITY,
        }
    }

    /// Creates a new [`DedupeEncoder`] with the specified capacity.
    ///
    /// The encoder will be able to hold at least `capacity` unique values and `num_types`
    /// categories of types without reallocating.
    #[inline(always)]
    pub fn with_capacity(initial_capacity: usize, num_types: usize) -> Self {
        Self {
            type_stores: HashMap::with_capacity(num_types),
            next_id: 1,
            initial_capacity,
        }
    }

    /// Removes all cached entries and resets assigned IDs.
    #[inline(always)]
    pub fn clear(&mut self) {
        self.type_stores.clear();
        self.next_id = 1;
    }

    /// Returns the number of unique values currently stored in the encoder (seen so far).
    #[inline(always)]
    pub const fn len(&self) -> usize {
        self.next_id - 1
    }

    /// Returns `true` if no values have been seen yet.
    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.next_id == 1
    }

    /// Encodes a value with deduplication.
    ///
    /// If the value has been seen before, only its ID is encoded. Otherwise, the value is
    /// encoded in full, preceded by a special ID (0).
    ///
    /// # Arguments
    ///
    /// * `val` - The value to encode. It must implement `Hash`, `Eq`, and `Pack`.
    /// * `writer` - The writer to which the encoded data will be written.
    ///
    /// # Returns
    ///
    /// The number of bytes written to the writer. Encodes `val` with deduplication support.
    ///
    /// When the value is first seen, this writes a special ID `0` followed by the packed
    /// value. On subsequent occurrences, only the assigned ID is written.
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
            .or_insert_with(|| {
                smallbox::smallbox!(HashMap::<T, usize>::with_capacity(self.initial_capacity))
            });

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

        // Store in type-specific map
        typed_store.insert(val.clone(), new_id);

        // Encode as new value (ID 0 followed by the actual value)
        let mut total_bytes = 0;
        total_bytes += Lencode::encode_varint(0usize, writer)?; // Special ID for new values
        total_bytes += val.pack(writer)?;
        Ok(total_bytes)
    }
}

#[derive(Default)]
/// Companion to [`DedupeEncoder`] that reconstructs repeated values from IDs.
pub struct DedupeDecoder {
    // Store values in order - index 0 = ID 1, index 1 = ID 2, etc.
    values: Vec<Box<dyn Any + Send + Sync>>,
}

impl DedupeDecoder {
    /// Creates a new empty `DedupeDecoder`.
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            values: Vec::with_capacity(DEFAULT_INITIAL_CAPACITY),
        }
    }

    /// Creates a new [`DedupeDecoder`] with the specified capacity.
    ///
    /// The decoder will be able to hold at least `capacity` cached values without
    /// reallocating. Creates a decoder with a pre‑allocated value table of `capacity`.
    #[inline(always)]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            values: Vec::with_capacity(capacity),
        }
    }

    /// Clears cached values.
    #[inline(always)]
    pub fn clear(&mut self) {
        self.values.clear();
    }

    /// Returns the number of cached values.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns `true` if the cache is empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Decodes a value with deduplication.
    ///
    /// If the ID is 0, a new value is decoded and stored in the table. Otherwise, the value is
    /// retrieved from the table using the given ID.
    ///
    /// # Arguments
    ///
    /// * `reader` - The reader from which the encoded data will be read.
    ///
    /// # Returns
    ///
    /// The decoded value. Decodes a value with deduplication support.
    ///
    /// If the next ID is `0`, a fresh value is decoded, stored, and returned. Otherwise, the
    /// referenced value is loaded from the cache.
    #[inline]
    pub fn decode<T: Pack + Clone + Hash + Eq + Send + Sync + 'static>(
        &mut self,
        reader: &mut impl Read,
    ) -> Result<T> {
        let id = Lencode::decode_varint::<usize>(reader)?;

        if id == 0 {
            // New value, decode it and store in table
            let value = T::unpack(reader)?;

            // Store the value (Vec index = ID - 1)
            self.values.push(Box::new(value.clone()));

            Ok(value)
        } else {
            // Existing value, retrieve from table
            let index = id - 1; // Convert ID to Vec index
            if let Some(boxed_value) = self.values.get(index)
                && let Some(typed_value) = boxed_value.downcast_ref::<T>()
            {
                return Ok(typed_value.clone());
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
