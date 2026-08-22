//! Dedupe-aware encoding: replaces repeated values with compact integer IDs.
//!
//! [`DedupeEncoder`] and [`DedupeDecoder`] maintain matching value→ID tables
//! so that the second (and later) occurrences of a value are emitted as a
//! short varint instead of the full payload. On the encoder side, the first
//! occurrence writes a marker `0` followed by the packed value; later
//! occurrences write just the value's assigned ID.
//!
//! # Priming
//!
//! [`DedupeEncoder::prime`] (and its decoder counterpart) lets you pre-register
//! a set of "known" values *without* placing them on the wire. Both sides
//! must prime the same sequence. After priming, [`DedupeEncoder::encode`]
//! calls for those values emit only the short ID — zero bytes of value
//! payload — which is ideal for encoding domain values with a long tail of
//! global popularity (e.g. widely referenced pubkeys, program IDs, etc.).
//!
//! # Frozen state (two-layer design)
//!
//! For workloads that need to *repeatedly* prime the same large set of values
//! at boundary points (e.g. per-slot in a streaming replay), rebuilding the
//! prime table each time is expensive. Instead, build it once and share it:
//!
//! 1. Prime a fresh [`DedupeEncoder`] / [`DedupeDecoder`] with all known
//!    values.
//! 2. Call [`DedupeEncoder::freeze`] / [`DedupeDecoder::freeze`] to convert
//!    it into an immutable [`FrozenEncoderState`] / [`FrozenDecoderState`].
//! 3. Wrap in [`Arc`] and hand out to as many workers as needed.
//! 4. Each worker creates a fresh encoder/decoder via
//!    [`DedupeEncoder::with_frozen`] / [`DedupeDecoder::with_frozen`]. The
//!    frozen layer is shared (via [`Arc`]); only a small per-instance
//!    scratch layer is private.
//! 5. Between boundary points, call [`DedupeEncoder::clear`] /
//!    [`DedupeDecoder::clear`]. This discards only the scratch layer;
//!    frozen lookups stay available at zero cost.
//!
//! ## Example
//!
//! ```
//! # #[cfg(feature = "std")] {
//! use std::sync::Arc;
//! use lencode::dedupe::{DedupeEncoder, DedupeDecoder, DefaultDedupeHasher};
//! use lencode::io::Cursor;
//!
//! // 1. Build & freeze the primed state once.
//! let mut primer_enc = DedupeEncoder::new();
//! let mut primer_dec = DedupeDecoder::new();
//! for &pubkey in &[1000u32, 2000, 3000] {
//!     primer_enc.prime::<u32, DefaultDedupeHasher>(&pubkey);
//!     primer_dec.prime::<u32>(pubkey);
//! }
//! let frozen_enc = Arc::new(primer_enc.freeze());
//! let frozen_dec = Arc::new(primer_dec.freeze());
//!
//! // 2. Workers reuse the frozen state.
//! let mut enc = DedupeEncoder::with_frozen(Arc::clone(&frozen_enc));
//! let mut dec = DedupeDecoder::with_frozen(Arc::clone(&frozen_dec));
//!
//! // 3. Encode a mix of primed and novel values.
//! let mut buffer = Vec::new();
//! enc.encode::<u32, DefaultDedupeHasher>(&2000, &mut buffer).unwrap(); // primed → 1-byte ID
//! enc.encode::<u32, DefaultDedupeHasher>(&99_999, &mut buffer).unwrap(); // novel → ID 0 + payload
//!
//! // 4. Between "slots", clear() resets scratch but keeps frozen.
//! enc.clear();
//! dec.clear();
//! # }
//! ```

use core::any::{Any, TypeId};
use core::hash::{BuildHasher, Hash};
use hashbrown::HashMap;
use smallbox::SmallBox;
use smallbox::space::S8;

/// Default [`BuildHasher`] used by [`DedupeEncodeable`] / [`DedupeDecodeable`]
/// implementations that don't override it.
pub type DefaultDedupeHasher = ahash::RandomState;

#[cfg(not(feature = "std"))]
use alloc::{boxed::Box, sync::Arc};
#[cfg(feature = "std")]
use std::{boxed::Box, sync::Arc};

use crate::prelude::*;

const DEFAULT_INITIAL_CAPACITY: usize = 128;
const DEFAULT_NUM_TYPES: usize = 4;

trait TypedVecStore: Any + Send + Sync {
    fn clear(&mut self);
    fn into_boxed_values(self: Box<Self>) -> Vec<Box<dyn Any + Send + Sync>>;
}

impl<T: Send + Sync + 'static> TypedVecStore for Vec<T> {
    #[inline]
    fn clear(&mut self) {
        Vec::clear(self);
    }

    #[inline]
    fn into_boxed_values(self: Box<Self>) -> Vec<Box<dyn Any + Send + Sync>> {
        (*self)
            .into_iter()
            .map(|value| Box::new(value) as Box<dyn Any + Send + Sync>)
            .collect()
    }
}

type TypedVec = (TypeId, Box<dyn TypedVecStore>);

type ClearTypeStore = fn(&mut (dyn Any + Send + Sync));

struct TypeStore {
    type_id: TypeId,
    values: SmallBox<dyn Any + Send + Sync, S8>,
    clear: ClearTypeStore,
    active: bool,
}

#[inline]
fn clear_type_store<T: 'static, S: 'static>(store: &mut (dyn Any + Send + Sync)) {
    store
        .downcast_mut::<HashMap<T, usize, S>>()
        .expect("typed dedupe map must match its TypeId")
        .clear();
}

/// Immutable, shareable snapshot of a primed [`DedupeEncoder`].
///
/// Built by calling [`DedupeEncoder::freeze`] on an encoder that has been
/// populated via [`DedupeEncoder::prime`]. Wrap in [`Arc`] to share across
/// many encoder instances — each new encoder created via
/// [`DedupeEncoder::with_frozen`] looks up values in this shared state
/// *before* its own scratch layer, paying zero per-instance setup cost.
pub struct FrozenEncoderState {
    // Per-type primed hashmaps, identical in shape to `DedupeEncoder::type_stores`.
    type_stores: Vec<TypeStore>,
    // Number of primed values; equal to the last-assigned ID.
    total_primed: usize,
}

/// Immutable, shareable snapshot of a primed [`DedupeDecoder`].
///
/// Companion to [`FrozenEncoderState`]; see [`DedupeDecoder::freeze`] and
/// [`DedupeDecoder::with_frozen`].
pub struct FrozenDecoderState {
    typed_vec: Option<TypedVec>,
    boxed_values: Vec<Box<dyn Any + Send + Sync>>,
    total_primed: usize,
}

impl FrozenEncoderState {
    /// Returns the total number of primed values.
    #[inline(always)]
    pub const fn len(&self) -> usize {
        self.total_primed
    }

    /// Returns `true` if no values are primed.
    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.total_primed == 0
    }
}

impl FrozenDecoderState {
    /// Returns the total number of primed values.
    #[inline(always)]
    pub const fn len(&self) -> usize {
        self.total_primed
    }

    /// Returns `true` if no values are primed.
    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.total_primed == 0
    }
}

/// Marker trait for types eligible for deduplicated encoding.
///
/// Types must be hashable, equatable, clonable and packable so they can be
/// stored in the encoder’s table and written on first occurrence.
/// Implement this with a blanket `impl` for your type when you want
/// [`Encode::encode_ext`] to take advantage of [`DedupeEncoder`].
pub trait DedupeEncodeable: Hash + Eq + Pack + Clone + Send + Sync + 'static {
    /// `BuildHasher` used for the per-type dedupe table. Override for types
    /// (like cryptographic keys) that benefit from a specialized hasher.
    type Hasher: BuildHasher + Default + Send + Sync + 'static;
}

/// Blanket [`Encode`] impl for all [`DedupeEncodeable`] types.
///
/// Delegates to [`DedupeEncoder::encode`] when a dedupe context is active,
/// otherwise falls back to [`Pack::pack`]. The [`Encode::encode_slice`]
/// override delegates to [`Pack::pack_slice`] for bulk encoding.
impl<T: DedupeEncodeable> Encode for T {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        ctx: Option<&mut crate::context::EncoderContext>,
    ) -> Result<usize> {
        if let Some(ctx) = ctx
            && let Some(encoder) = ctx.dedupe.as_mut()
        {
            return encoder.encode::<T, T::Hasher>(self, writer);
        }
        self.pack(writer)
    }

    #[inline(always)]
    fn encode_slice(items: &[Self], writer: &mut impl Write) -> Result<usize> {
        T::pack_slice(items, writer)
    }

    #[inline(always)]
    fn encode_slice_ext(
        items: &[Self],
        writer: &mut impl Write,
        ctx: Option<&mut crate::context::EncoderContext>,
    ) -> Result<usize> {
        if let Some(ctx) = ctx
            && let Some(encoder) = ctx.dedupe.as_mut()
        {
            return encoder.encode_many::<T, T::Hasher>(items, writer);
        }
        T::pack_slice(items, writer)
    }
}

/// Marker trait for types eligible for deduplicated decoding.
///
/// Pairs with `DedupeEncodeable`; see that trait for details.
pub trait DedupeDecodeable: Pack + Clone + Hash + Eq + Send + Sync + 'static {
    /// Mirrors [`DedupeEncodeable::Hasher`]. Must match the encoder side for a
    /// given type; the decoder doesn't actually hash, but keeping the symmetry
    /// makes it harder to wire mismatched halves together.
    type Hasher: BuildHasher + Default + Send + Sync + 'static;
}

/// Blanket [`Decode`] impl for all [`DedupeDecodeable`] types.
///
/// Delegates to [`DedupeDecoder::decode`] when a dedupe context is active,
/// otherwise falls back to [`Pack::unpack`]. The [`Decode::decode_vec`]
/// override delegates to [`Pack::unpack_vec`] for bulk decoding.
impl<T: DedupeDecodeable> Decode for T {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        ctx: Option<&mut crate::context::DecoderContext>,
    ) -> Result<Self> {
        if let Some(ctx) = ctx
            && let Some(decoder) = ctx.dedupe.as_mut()
        {
            return decoder.decode::<T>(reader);
        }
        T::unpack(reader)
    }

    #[inline(always)]
    fn decode_vec(reader: &mut impl Read, count: usize) -> Result<Vec<Self>> {
        T::unpack_vec(reader, count)
    }

    #[inline(always)]
    fn decode_vec_ext(
        reader: &mut impl Read,
        count: usize,
        ctx: Option<&mut crate::context::DecoderContext>,
    ) -> Result<Vec<Self>> {
        if let Some(ctx) = ctx
            && let Some(decoder) = ctx.dedupe.as_mut()
        {
            return decoder.decode_many::<T>(reader, count);
        }
        T::unpack_vec(reader, count)
    }
}

/// Stateful encoder that replaces repeated values with compact IDs.
///
/// When backed by a [`FrozenEncoderState`] (via [`Self::with_frozen`]), lookups
/// check the immutable frozen table first before consulting the per-instance
/// scratch layer. [`Self::clear`] only clears the scratch layer, so the frozen
/// portion is preserved across slot boundaries at effectively zero cost.
pub struct DedupeEncoder {
    // Shared immutable primed state. Lookups check this first.
    frozen: Option<Arc<FrozenEncoderState>>,
    // Cached copy of `frozen.total_primed` (0 if no frozen state). Avoids an
    // Arc deref in `clear()` and keeps the post-clear `next_id` computation
    // branch-free.
    frozen_total_primed: usize,
    // Per-type hashmaps stored as a small Vec for linear-search lookup by TypeId.
    // Typical workloads use 1–4 types, where a linear scan over a Vec is
    // significantly faster than hashing a TypeId through a HashMap.
    // Contains scratch (non-frozen) values only.
    type_stores: Vec<TypeStore>,
    // Next ID to assign. Starts at 1 with no frozen state, or at
    // `frozen.total_primed + 1` when frozen.
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
            frozen: None,
            frozen_total_primed: 0,
            type_stores: Vec::with_capacity(DEFAULT_NUM_TYPES),
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
            frozen: None,
            frozen_total_primed: 0,
            type_stores: Vec::with_capacity(num_types),
            next_id: 1,
            initial_capacity,
        }
    }

    /// Creates a new `DedupeEncoder` backed by a shared immutable frozen state.
    ///
    /// The encoder's lookup path checks the frozen state first, then its own
    /// scratch layer. Novel values are assigned IDs starting after the frozen
    /// range, and [`Self::clear`] resets only the scratch layer — the frozen
    /// state is untouched.
    ///
    /// Build the frozen state once at program startup by calling
    /// [`Self::prime`] followed by [`Self::freeze`], then share it via
    /// [`Arc`] across as many worker encoders as you need.
    #[inline(always)]
    pub fn with_frozen(frozen: Arc<FrozenEncoderState>) -> Self {
        let total_primed = frozen.total_primed;
        Self {
            frozen: Some(frozen),
            frozen_total_primed: total_primed,
            type_stores: Vec::with_capacity(DEFAULT_NUM_TYPES),
            next_id: total_primed + 1,
            initial_capacity: DEFAULT_INITIAL_CAPACITY,
        }
    }

    /// Consumes this encoder and produces an immutable snapshot suitable for
    /// sharing across worker encoders.
    ///
    /// Call this after priming a fresh encoder via [`Self::prime`]. Values
    /// added via [`Self::encode`] are captured into the snapshot too, which
    /// is usually not what you want — prime before freezing.
    ///
    /// # Panics
    ///
    /// Panics if `self` was constructed via [`Self::with_frozen`]. A frozen
    /// encoder's scratch layer is not safe to freeze because its IDs start
    /// after the existing frozen range.
    pub fn freeze(mut self) -> FrozenEncoderState {
        assert!(
            self.frozen.is_none(),
            "cannot freeze an encoder that already has a frozen state"
        );
        self.type_stores.retain(|store| store.active);
        FrozenEncoderState {
            type_stores: self.type_stores,
            total_primed: self.next_id - 1,
        }
    }

    /// Removes all scratch entries and resets assigned IDs.
    ///
    /// If the encoder has a frozen state, `next_id` is reset to
    /// `frozen.total_primed + 1` so subsequent novel values are still assigned
    /// unique IDs above the frozen range. Frozen entries are never cleared.
    #[inline(always)]
    pub fn clear(&mut self) {
        for store in &mut self.type_stores {
            if store.active {
                (store.clear)(&mut *store.values);
                store.active = false;
            }
        }
        self.next_id = self.frozen_total_primed + 1;
    }

    /// Returns the number of unique values currently stored in the encoder
    /// (including frozen values, if any).
    #[inline(always)]
    pub const fn len(&self) -> usize {
        self.next_id - 1
    }

    /// Returns `true` if no values have been seen yet.
    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.next_id == 1
    }

    /// Returns the number of distinct types that have been stored.
    #[inline(always)]
    pub fn num_types(&self) -> usize {
        self.type_stores.iter().filter(|store| store.active).count()
    }

    /// Returns an iterator over the [`TypeId`]s of all stored types.
    #[inline(always)]
    pub fn type_ids(&self) -> impl Iterator<Item = TypeId> + '_ {
        self.type_stores
            .iter()
            .filter_map(|store| store.active.then_some(store.type_id))
    }

    /// Returns `true` if any entries exist for type `T`.
    #[inline]
    pub fn contains_type<T: 'static>(&self) -> bool {
        let type_id = TypeId::of::<T>();
        self.type_stores
            .iter()
            .any(|store| store.active && store.type_id == type_id)
    }

    /// Returns the number of unique values stored for type `T`.
    ///
    /// Returns `0` if no values of type `T` have been seen. The hasher type
    /// `S` must match the one used when encoding values of `T`.
    #[inline]
    pub fn len_for_type<T, S>(&self) -> usize
    where
        T: Hash + Eq + Send + Sync + 'static,
        S: BuildHasher + Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();
        self.type_stores
            .iter()
            .find(|store| store.active && store.type_id == type_id)
            .and_then(|store| store.values.downcast_ref::<HashMap<T, usize, S>>())
            .map_or(0, |m| m.len())
    }

    /// Returns an iterator over the unique values stored for type `T`.
    ///
    /// Returns an empty iterator if no values of type `T` have been seen. The
    /// hasher type `S` must match the one used when encoding values of `T`.
    #[inline]
    pub fn values_for_type<T, S>(&self) -> impl Iterator<Item = &T>
    where
        T: Hash + Eq + Send + Sync + 'static,
        S: BuildHasher + Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();
        self.type_stores
            .iter()
            .find(|store| store.active && store.type_id == type_id)
            .and_then(|store| store.values.downcast_ref::<HashMap<T, usize, S>>())
            .into_iter()
            .flat_map(|m| m.keys())
    }

    /// Removes all cached entries for a specific type `T`.
    ///
    /// Other types' entries and their IDs are unaffected.
    /// **Warning:** clearing a single type invalidates existing IDs for that type,
    /// so the encoder and decoder must be kept in sync.
    #[inline]
    pub fn clear_type<T: Hash + Eq + Send + Sync + 'static>(&mut self) {
        let type_id = TypeId::of::<T>();
        if let Some(pos) = self
            .type_stores
            .iter()
            .position(|store| store.type_id == type_id)
        {
            self.type_stores.swap_remove(pos);
        }
    }

    /// Returns an estimate of the heap memory (in bytes) used by the encoder's
    /// internal tables.
    ///
    /// This is a rough lower bound: it accounts for the vec overhead and
    /// stored key/value sizes but not allocator metadata.
    #[inline]
    pub const fn memory_usage(&self) -> usize {
        use core::mem::size_of;
        let mut total = self.type_stores.capacity() * size_of::<TypeStore>();

        let entry_count = self.len();
        total += entry_count * size_of::<usize>() * 3;

        total
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
    pub fn encode<T, S>(&mut self, val: &T, writer: &mut impl Write) -> Result<usize>
    where
        T: Hash + Eq + Pack + Clone + Send + Sync + 'static,
        S: BuildHasher + Default + Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();

        // Check frozen (immutable) state first, if present. Most real-world
        // hits for the horizon use case land here.
        if let Some(frozen) = &self.frozen
            && let Some(store) = frozen
                .type_stores
                .iter()
                .find(|store| store.type_id == type_id)
        {
            // SAFETY: same invariant as in scratch path — slot was inserted as
            // HashMap<T, usize, S>.
            let typed_store: &HashMap<T, usize, S> = unsafe {
                &*(&*store.values as *const (dyn Any + Send + Sync) as *const HashMap<T, usize, S>)
            };
            if let Some(&existing_id) = typed_store.get(val) {
                return Lencode::encode_varint_u64(existing_id as u64, writer);
            }
        }

        // Linear scan for the type-specific scratch store. For the typical 1–4 types
        // this is faster than hashing a TypeId through a HashMap.
        let store = match self
            .type_stores
            .iter_mut()
            .find(|store| store.type_id == type_id)
        {
            Some(store) => {
                if !store.active {
                    store.active = true;
                }
                &mut store.values
            }
            None => {
                self.type_stores.push(TypeStore {
                    type_id,
                    values: smallbox::smallbox!(HashMap::<T, usize, S>::with_capacity_and_hasher(
                        self.initial_capacity,
                        S::default(),
                    )),
                    clear: clear_type_store::<T, S>,
                    active: true,
                });
                &mut self.type_stores.last_mut().unwrap().values
            }
        };

        // SAFETY: we just matched `type_id == TypeId::of::<T>()` in the linear
        // scan, and this slot was originally inserted with a
        // `HashMap::<T, usize, S>`. Skipping `downcast_mut` avoids a redundant
        // vtable call to `type_id()`.
        let typed_store: &mut HashMap<T, usize, S> = unsafe {
            &mut *(&mut **store as *mut (dyn Any + Send + Sync) as *mut HashMap<T, usize, S>)
        };

        // Check if we've already seen this value in scratch
        if let Some(&existing_id) = typed_store.get(val) {
            // Value has been seen before, encode its ID
            return Lencode::encode_varint_u64(existing_id as u64, writer);
        }

        // New value - assign an ID and store it
        let new_id = self.next_id;
        self.next_id += 1;

        // Store in type-specific map
        typed_store.insert(val.clone(), new_id);

        // Encode as new value (ID 0 followed by the actual value)
        let mut total_bytes = 0;
        total_bytes += Lencode::encode_varint_u64(0, writer)?; // Special ID for new values
        total_bytes += val.pack(writer)?;
        Ok(total_bytes)
    }

    /// Encodes a homogeneous collection after resolving its frozen and
    /// scratch maps once. The scratch map remains inactive when every value
    /// is frozen, matching the per-value path's externally visible state.
    #[inline(never)]
    fn encode_many<T, S>(&mut self, values: &[T], writer: &mut impl Write) -> Result<usize>
    where
        T: Hash + Eq + Pack + Clone + Send + Sync + 'static,
        S: BuildHasher + Default + Send + Sync + 'static,
    {
        if values.is_empty() {
            return Ok(0);
        }

        if self.frozen.is_none() {
            // Match plain homogeneous collection encoding: one estimated
            // allocation avoids repeatedly copying the growing output prefix.
            writer.reserve(core::mem::size_of_val(values) + 9);
        }

        let type_id = TypeId::of::<T>();
        let frozen_store = self.frozen.as_ref().and_then(|frozen| {
            frozen
                .type_stores
                .iter()
                .find(|store| store.type_id == type_id)
                .map(|store| {
                    store
                        .values
                        .downcast_ref::<HashMap<T, usize, S>>()
                        .expect("typed frozen map must match its TypeId")
                })
        });

        let initial_capacity = self.initial_capacity;
        let store = match self
            .type_stores
            .iter_mut()
            .find(|store| store.type_id == type_id)
        {
            Some(store) => store,
            None => {
                self.type_stores.push(TypeStore {
                    type_id,
                    values: smallbox::smallbox!(HashMap::<T, usize, S>::with_capacity_and_hasher(
                        initial_capacity,
                        S::default(),
                    )),
                    clear: clear_type_store::<T, S>,
                    active: false,
                });
                self.type_stores.last_mut().unwrap()
            }
        };
        let active = &mut store.active;
        let scratch_store = store
            .values
            .downcast_mut::<HashMap<T, usize, S>>()
            .expect("typed scratch map must match its TypeId");
        let next_id = &mut self.next_id;

        let mut total_bytes = 0;
        for value in values {
            if let Some(existing_id) = frozen_store.and_then(|store| store.get(value)).copied() {
                total_bytes += Lencode::encode_varint_u64(existing_id as u64, writer)?;
                continue;
            }
            if let Some(existing_id) = scratch_store.get(value).copied() {
                total_bytes += Lencode::encode_varint_u64(existing_id as u64, writer)?;
                continue;
            }

            *active = true;
            let new_id = *next_id;
            *next_id += 1;
            scratch_store.insert(value.clone(), new_id);
            total_bytes += Lencode::encode_varint_u64(0, writer)?;
            total_bytes += value.pack(writer)?;
        }
        Ok(total_bytes)
    }

    /// Pre-populates the encoder's table with a value without writing to any
    /// output stream.
    ///
    /// Priming lets encoder and decoder agree on a set of "known" values up
    /// front, so subsequent [`encode`](Self::encode) calls for those values
    /// emit only their compact ID — the full value is never placed on the
    /// wire. The caller is responsible for calling the matching
    /// [`DedupeDecoder::prime`] with the exact same sequence of values on the
    /// decode side so IDs line up.
    ///
    /// If `val` has already been registered (via a previous `prime` or
    /// `encode`), this is a no-op and the existing ID is returned. Otherwise
    /// a fresh ID is assigned and returned.
    ///
    /// # Panics
    ///
    /// Panics if called on an encoder that was constructed via
    /// [`Self::with_frozen`]. Priming is intended to be done up-front on a
    /// fresh encoder before calling [`Self::freeze`] — once a frozen state
    /// exists, subsequent primes would collide with the existing ID range.
    #[inline]
    pub fn prime<T, S>(&mut self, val: &T) -> usize
    where
        T: Hash + Eq + Pack + Clone + Send + Sync + 'static,
        S: BuildHasher + Default + Send + Sync + 'static,
    {
        assert!(
            self.frozen.is_none(),
            "cannot prime an encoder that already has a frozen state"
        );
        let type_id = TypeId::of::<T>();

        let store = match self
            .type_stores
            .iter_mut()
            .find(|store| store.type_id == type_id)
        {
            Some(store) => {
                if !store.active {
                    store.active = true;
                }
                &mut store.values
            }
            None => {
                self.type_stores.push(TypeStore {
                    type_id,
                    values: smallbox::smallbox!(HashMap::<T, usize, S>::with_capacity_and_hasher(
                        self.initial_capacity,
                        S::default(),
                    )),
                    clear: clear_type_store::<T, S>,
                    active: true,
                });
                &mut self.type_stores.last_mut().unwrap().values
            }
        };

        // SAFETY: TypeId matches and the slot was inserted as HashMap<T, usize, S>.
        let typed_store: &mut HashMap<T, usize, S> = unsafe {
            &mut *(&mut **store as *mut (dyn Any + Send + Sync) as *mut HashMap<T, usize, S>)
        };

        if let Some(&existing_id) = typed_store.get(val) {
            return existing_id;
        }

        let new_id = self.next_id;
        self.next_id += 1;
        typed_store.insert(val.clone(), new_id);
        new_id
    }
}

/// Companion to [`DedupeEncoder`] that reconstructs repeated values from IDs.
///
/// Internally uses a type‑erased `Vec<T>` for single‑type workloads (the common
/// case), storing values inline without per‑value Box allocations. Falls back to
/// `Vec<Box<dyn Any>>` when multiple types are decoded.
///
/// When backed by a [`FrozenDecoderState`] (via [`Self::with_frozen`]), lookups
/// check the immutable frozen table first before the scratch layer.
/// [`Self::clear`] only clears the scratch layer.
pub struct DedupeDecoder {
    // Shared immutable primed state.
    frozen: Option<Arc<FrozenDecoderState>>,
    // Cached copy of `frozen.total_primed` (0 if no frozen state). Kept in sync
    // with `frozen` so const methods like `len()` can stay const without Arc
    // deref on the hot path.
    frozen_total_primed: usize,
    // Type-erased Vec<T> for the current (or only) decoded type.
    // Contains only scratch (novel) values.
    typed_vec: Option<TypedVec>,
    // Fallback for multi-type scenarios: scratch values stored as Box<dyn Any>.
    boxed_values: Vec<Box<dyn Any + Send + Sync>>,
    // Count of values in the scratch layer only.
    scratch_count: usize,
}

impl Default for DedupeDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl DedupeDecoder {
    /// Moves the single-type scratch store into the global-ID-ordered erased
    /// store before a second value type is introduced.
    #[inline]
    fn promote_scratch_to_boxed(&mut self) {
        let Some((_, store)) = self.typed_vec.take() else {
            return;
        };
        debug_assert!(self.boxed_values.is_empty());
        self.boxed_values = store.into_boxed_values();
        debug_assert_eq!(self.boxed_values.len(), self.scratch_count);
    }

    /// Creates a new empty `DedupeDecoder`.
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            frozen: None,
            frozen_total_primed: 0,
            typed_vec: None,
            boxed_values: Vec::new(),
            scratch_count: 0,
        }
    }

    /// Creates a new [`DedupeDecoder`] with the specified capacity.
    #[inline(always)]
    pub fn with_capacity(capacity: usize) -> Self {
        let _ = capacity;
        Self {
            frozen: None,
            frozen_total_primed: 0,
            typed_vec: None,
            boxed_values: Vec::new(),
            scratch_count: 0,
        }
    }

    /// Creates a new `DedupeDecoder` backed by a shared immutable frozen state.
    ///
    /// The decoder's lookup path checks the frozen state first, then its own
    /// scratch layer. Novel values (with ID 0 on the wire) land in scratch.
    /// [`Self::clear`] resets only the scratch layer.
    #[inline(always)]
    pub fn with_frozen(frozen: Arc<FrozenDecoderState>) -> Self {
        let frozen_total_primed = frozen.total_primed;
        Self {
            frozen: Some(frozen),
            frozen_total_primed,
            typed_vec: None,
            boxed_values: Vec::new(),
            scratch_count: 0,
        }
    }

    /// Consumes this decoder and produces an immutable snapshot suitable for
    /// sharing across worker decoders.
    ///
    /// # Panics
    ///
    /// Panics if `self` was constructed via [`Self::with_frozen`].
    pub fn freeze(self) -> FrozenDecoderState {
        assert!(
            self.frozen.is_none(),
            "cannot freeze a decoder that already has a frozen state"
        );
        FrozenDecoderState {
            typed_vec: self.typed_vec,
            boxed_values: self.boxed_values,
            total_primed: self.scratch_count,
        }
    }

    /// Clears cached scratch values. Frozen values are preserved.
    #[inline(always)]
    pub fn clear(&mut self) {
        if let Some((_, store)) = self.typed_vec.as_mut() {
            store.clear();
        }
        self.boxed_values.clear();
        self.scratch_count = 0;
    }

    /// Returns the total number of cached values (frozen + scratch).
    #[inline(always)]
    pub const fn len(&self) -> usize {
        self.frozen_total_primed + self.scratch_count
    }

    /// Returns `true` if the cache is empty.
    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns an estimate of the heap memory (in bytes) used by the decoder's
    /// scratch value cache.
    #[inline]
    pub fn memory_usage(&self) -> usize {
        use core::mem::size_of;
        self.boxed_values.capacity() * size_of::<Box<dyn Any + Send + Sync>>()
            + self
                .typed_vec
                .as_ref()
                .map_or(0, |_| self.scratch_count * 64)
    }

    /// Decodes a value with deduplication.
    ///
    /// If the next ID is `0`, a fresh value is decoded, stored in scratch, and
    /// returned. Otherwise the referenced value is loaded from the frozen
    /// state (if the ID falls within the frozen range) or from scratch.
    #[inline]
    pub fn decode<T: Pack + Clone + Hash + Eq + Send + Sync + 'static>(
        &mut self,
        reader: &mut impl Read,
    ) -> Result<T> {
        let id = usize::try_from(Lencode::decode_varint_u64(reader)?)
            .map_err(|_| crate::io::Error::DecodeLimitExceeded)?;
        let type_id = TypeId::of::<T>();
        let total_primed = self.frozen_total_primed;

        if id != 0 && id <= total_primed {
            let frozen = self.frozen.as_ref().unwrap();
            return lookup_frozen::<T>(frozen, type_id, id - 1);
        }

        let scratch_id_base = total_primed;

        if let Some((ref cached_type, ref mut store)) = self.typed_vec
            && *cached_type == type_id
        {
            // SAFETY: TypeId matches, so the erased store holds Vec<T>.
            let vec: &mut Vec<T> =
                unsafe { &mut *(store.as_mut() as *mut dyn TypedVecStore as *mut Vec<T>) };
            if id == 0 {
                let value = T::unpack(reader)?;
                reader.claim_allocation(core::mem::size_of::<T>())?;
                vec.push(value.clone());
                self.scratch_count += 1;
                return Ok(value);
            }
            let index = id - scratch_id_base - 1;
            return vec.get(index).cloned().ok_or(crate::io::Error::InvalidData);
        }

        if self.scratch_count == 0 && self.boxed_values.is_empty() {
            if id != 0 {
                return Err(crate::io::Error::InvalidData);
            }
            let value = T::unpack(reader)?;
            reader.claim_allocation(core::mem::size_of::<T>())?;
            let mut vec = Vec::with_capacity(DEFAULT_INITIAL_CAPACITY);
            vec.push(value.clone());
            self.typed_vec = Some((type_id, Box::new(vec)));
            self.scratch_count += 1;
            return Ok(value);
        }

        self.promote_scratch_to_boxed();
        if id == 0 {
            let value = T::unpack(reader)?;
            reader.claim_allocation(
                core::mem::size_of::<T>() + core::mem::size_of::<Box<dyn Any + Send + Sync>>(),
            )?;
            self.boxed_values.push(Box::new(value.clone()));
            self.scratch_count += 1;
            Ok(value)
        } else {
            let index = id - scratch_id_base - 1;
            self.boxed_values
                .get(index)
                .and_then(|value| value.downcast_ref::<T>())
                .cloned()
                .ok_or(crate::io::Error::InvalidData)
        }
    }

    /// Decodes a deduplicated value into decoder-owned storage and borrows it.
    ///
    /// Frozen and repeated values are returned without copying. Novel values
    /// are unpacked directly into scratch storage, also avoiding the clone that
    /// the owned [`Self::decode`] API requires. The borrow remains valid until
    /// the next mutable access to this decoder.
    #[inline]
    pub fn decode_ref<'a, T: Pack + Hash + Eq + Send + Sync + 'static>(
        &'a mut self,
        reader: &mut impl Read,
    ) -> Result<&'a T> {
        let id = usize::try_from(Lencode::decode_varint_u64(reader)?)
            .map_err(|_| crate::io::Error::DecodeLimitExceeded)?;
        let type_id = TypeId::of::<T>();
        let total_primed = self.frozen_total_primed;

        // Frozen lookup: id in 1..=total_primed refers to a primed value.
        if id != 0 && id <= total_primed {
            // SAFETY: total_primed > 0 implies frozen is Some (set together in
            // `with_frozen`). Avoids an extra option-check on the hot path.
            let frozen = self.frozen.as_ref().unwrap();
            return lookup_frozen_ref::<T>(frozen, type_id, id - 1);
        }

        // Scratch indices are 0-based starting after the frozen range.
        let scratch_id_base = total_primed;

        // Fast path: typed Vec<T> for single-type workloads.
        if let Some((ref cached_type, ref mut store)) = self.typed_vec
            && *cached_type == type_id
        {
            // SAFETY: we verified TypeId matches; the store holds a Vec<T>.
            let vec: &mut Vec<T> =
                unsafe { &mut *(store.as_mut() as *mut dyn TypedVecStore as *mut Vec<T>) };
            if id == 0 {
                let value = T::unpack(reader)?;
                reader.claim_allocation(core::mem::size_of::<T>())?;
                vec.push(value);
                self.scratch_count += 1;
                return Ok(vec.last().expect("value was just pushed"));
            } else {
                let index = id - scratch_id_base - 1;
                if let Some(v) = vec.get(index) {
                    return Ok(v);
                }
                return Err(crate::io::Error::InvalidData);
            }
        }

        // First call for this type: initialize the typed Vec
        if self.scratch_count == 0 && self.boxed_values.is_empty() {
            let mut vec: Vec<T> = Vec::with_capacity(DEFAULT_INITIAL_CAPACITY);
            if id == 0 {
                let value = T::unpack(reader)?;
                reader.claim_allocation(core::mem::size_of::<T>())?;
                vec.push(value);
                self.typed_vec = Some((type_id, Box::new(vec)));
                self.scratch_count += 1;
                let (_, store) = self.typed_vec.as_ref().expect("store was just initialized");
                let store: &dyn Any = store.as_ref();
                return store
                    .downcast_ref::<Vec<T>>()
                    .and_then(|values| values.last())
                    .ok_or(crate::io::Error::InvalidData);
            } else {
                // Trying to reference a value before any were stored
                return Err(crate::io::Error::InvalidData);
            }
        }

        // A second type switches scratch storage to a global-ID-ordered erased
        // vector. Keeping every prior value in the vector is required because
        // IDs are shared across types.
        self.promote_scratch_to_boxed();
        if id == 0 {
            let value = T::unpack(reader)?;
            reader.claim_allocation(
                core::mem::size_of::<T>() + core::mem::size_of::<Box<dyn Any + Send + Sync>>(),
            )?;
            self.boxed_values.push(Box::new(value));
            self.scratch_count += 1;
            self.boxed_values
                .last()
                .and_then(|value| value.downcast_ref::<T>())
                .ok_or(crate::io::Error::InvalidData)
        } else {
            let index = id - scratch_id_base - 1;
            if let Some(boxed_value) = self.boxed_values.get(index) {
                return boxed_value
                    .downcast_ref::<T>()
                    .ok_or(crate::io::Error::InvalidData);
            }
            Err(crate::io::Error::InvalidData)
        }
    }

    /// Decodes a homogeneous collection after validating its erased frozen
    /// and scratch vectors once. Falls back to the general per-value path for
    /// empty, multi-type, or not-yet-initialized decoder states.
    #[inline(never)]
    fn decode_many<T: Pack + Clone + Hash + Eq + Send + Sync + 'static>(
        &mut self,
        reader: &mut impl Read,
        count: usize,
    ) -> Result<Vec<T>> {
        let type_id = TypeId::of::<T>();
        let scratch_compatible = self.boxed_values.is_empty()
            && self
                .typed_vec
                .as_ref()
                .is_some_and(|(cached_type, _)| *cached_type == type_id);
        let frozen_compatible = self.frozen_total_primed == 0
            || self.frozen.as_ref().is_some_and(|frozen| {
                frozen.boxed_values.is_empty()
                    && frozen
                        .typed_vec
                        .as_ref()
                        .is_some_and(|(cached_type, _)| *cached_type == type_id)
            });

        if count == 0 || !scratch_compatible || !frozen_compatible {
            let mut values = Vec::with_capacity(count);
            for _ in 0..count {
                values.push(self.decode::<T>(reader)?);
            }
            return Ok(values);
        }

        let total_primed = self.frozen_total_primed;
        let frozen_vec = if total_primed == 0 {
            None
        } else {
            let frozen = self
                .frozen
                .as_ref()
                .expect("primed decoder has frozen state");
            let (_, store) = frozen
                .typed_vec
                .as_ref()
                .expect("compatible frozen state has a typed vector");
            let store: &dyn Any = store.as_ref();
            Some(
                store
                    .downcast_ref::<Vec<T>>()
                    .expect("typed frozen vector must match its TypeId"),
            )
        };

        let (_, store) = self
            .typed_vec
            .as_mut()
            .expect("compatible scratch state has a typed vector");
        let store: &mut dyn Any = store.as_mut();
        let scratch_vec = store
            .downcast_mut::<Vec<T>>()
            .expect("typed scratch vector must match its TypeId");

        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            let id = usize::try_from(Lencode::decode_varint_u64(reader)?)
                .map_err(|_| crate::io::Error::DecodeLimitExceeded)?;
            if id != 0 && id <= total_primed {
                let value = frozen_vec
                    .and_then(|vec| vec.get(id - 1))
                    .ok_or(crate::io::Error::InvalidData)?;
                values.push(value.clone());
                continue;
            }
            if id == 0 {
                let value = T::unpack(reader)?;
                reader.claim_allocation(core::mem::size_of::<T>())?;
                scratch_vec.push(value.clone());
                self.scratch_count += 1;
                values.push(value);
                continue;
            }
            let value = scratch_vec
                .get(id - total_primed - 1)
                .ok_or(crate::io::Error::InvalidData)?;
            values.push(value.clone());
        }
        Ok(values)
    }

    /// Pre-populates the decoder's cache with a value without reading from any
    /// input stream.
    ///
    /// Pair this with [`DedupeEncoder::prime`] so both sides agree on the set
    /// of known values before decoding begins. Values must be primed in the
    /// exact same order on both sides for IDs to line up.
    ///
    /// # Panics
    ///
    /// Panics if called on a decoder that was constructed via
    /// [`Self::with_frozen`].
    #[inline]
    pub fn prime<T: Pack + Clone + Hash + Eq + Send + Sync + 'static>(&mut self, val: T) {
        assert!(
            self.frozen.is_none(),
            "cannot prime a decoder that already has a frozen state"
        );
        let type_id = TypeId::of::<T>();

        // Fast path: reuse the typed Vec
        if let Some((ref cached_type, ref mut store)) = self.typed_vec
            && *cached_type == type_id
        {
            // SAFETY: TypeId matches; store holds a Vec<T>.
            let vec: &mut Vec<T> =
                unsafe { &mut *(store.as_mut() as *mut dyn TypedVecStore as *mut Vec<T>) };
            vec.push(val);
            self.scratch_count += 1;
            return;
        }

        // First call for this type: initialize the typed Vec
        if self.scratch_count == 0 && self.boxed_values.is_empty() {
            let mut vec: Vec<T> = Vec::with_capacity(DEFAULT_INITIAL_CAPACITY);
            vec.push(val);
            self.typed_vec = Some((type_id, Box::new(vec)));
            self.scratch_count += 1;
            return;
        }

        // A second type must preserve the global ID positions assigned to the
        // first type before adding its value.
        self.promote_scratch_to_boxed();
        self.boxed_values.push(Box::new(val));
        self.scratch_count += 1;
    }
}

/// Clones a value at `vec_index` from frozen state for the owned decode path.
#[inline]
fn lookup_frozen<T: Clone + 'static>(
    frozen: &FrozenDecoderState,
    type_id: TypeId,
    vec_index: usize,
) -> Result<T> {
    if let Some((ref cached_type, ref store)) = frozen.typed_vec
        && *cached_type == type_id
    {
        // SAFETY: TypeId matches, so the erased store holds Vec<T>.
        let vec: &Vec<T> = unsafe { &*(&**store as *const dyn TypedVecStore as *const Vec<T>) };
        return vec
            .get(vec_index)
            .cloned()
            .ok_or(crate::io::Error::InvalidData);
    }
    frozen
        .boxed_values
        .get(vec_index)
        .and_then(|value| value.downcast_ref::<T>())
        .cloned()
        .ok_or(crate::io::Error::InvalidData)
}

/// Borrows a value at `vec_index` from frozen state for zero-copy callers.
#[inline]
fn lookup_frozen_ref<T: 'static>(
    frozen: &FrozenDecoderState,
    type_id: TypeId,
    vec_index: usize,
) -> Result<&T> {
    // Fast path: typed_vec
    if let Some((ref cached_type, ref store)) = frozen.typed_vec
        && *cached_type == type_id
    {
        // SAFETY: TypeId matches; store holds a Vec<T>.
        let vec: &Vec<T> = unsafe { &*(&**store as *const dyn TypedVecStore as *const Vec<T>) };
        if let Some(v) = vec.get(vec_index) {
            return Ok(v);
        }
        return Err(crate::io::Error::InvalidData);
    }
    // Multi-type fallback. The erased vector is global-ID ordered, and a
    // mismatched requested type is malformed input rather than a valid cast.
    if let Some(boxed_value) = frozen.boxed_values.get(vec_index) {
        return boxed_value
            .downcast_ref::<T>()
            .ok_or(crate::io::Error::InvalidData);
    }
    Err(crate::io::Error::InvalidData)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{DecoderContext, EncoderContext};
    use crate::io::Cursor;

    type H = DefaultDedupeHasher;

    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    struct BulkValue(u32);

    impl Pack for BulkValue {
        fn pack(&self, writer: &mut impl Write) -> Result<usize> {
            self.0.pack(writer)
        }

        fn unpack(reader: &mut impl Read) -> Result<Self> {
            u32::unpack(reader).map(Self)
        }
    }

    impl DedupeEncodeable for BulkValue {
        type Hasher = H;
    }

    impl DedupeDecodeable for BulkValue {
        type Hasher = H;
    }

    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    struct OtherBulkValue(u64);

    impl Pack for OtherBulkValue {
        fn pack(&self, writer: &mut impl Write) -> Result<usize> {
            self.0.pack(writer)
        }

        fn unpack(reader: &mut impl Read) -> Result<Self> {
            u64::unpack(reader).map(Self)
        }
    }

    impl DedupeEncodeable for OtherBulkValue {
        type Hasher = H;
    }

    impl DedupeDecodeable for OtherBulkValue {
        type Hasher = H;
    }

    #[derive(Debug, Eq, Hash, PartialEq)]
    struct NonCloneValue(u32);

    impl Pack for NonCloneValue {
        fn pack(&self, writer: &mut impl Write) -> Result<usize> {
            self.0.pack(writer)
        }

        fn unpack(reader: &mut impl Read) -> Result<Self> {
            u32::unpack(reader).map(Self)
        }
    }

    #[test]
    fn test_contextual_vec_decode_fast_path_roundtrip() {
        let primed = [BulkValue(10), BulkValue(20), BulkValue(30)];
        let mut primer_enc = DedupeEncoder::new();
        let mut primer_dec = DedupeDecoder::new();
        for value in &primed {
            primer_enc.prime::<BulkValue, H>(value);
            primer_dec.prime(value.clone());
        }

        let frozen_enc = Arc::new(primer_enc.freeze());
        let frozen_dec = Arc::new(primer_dec.freeze());
        let mut enc_ctx = EncoderContext {
            dedupe: Some(DedupeEncoder::with_frozen(frozen_enc)),
            diff: None,
        };
        let mut dec_ctx = DecoderContext {
            dedupe: Some(DedupeDecoder::with_frozen(frozen_dec)),
            diff: None,
        };
        let batches = [
            vec![BulkValue(20), BulkValue(40), BulkValue(40), BulkValue(10)],
            vec![BulkValue(30), BulkValue(50), BulkValue(10), BulkValue(50)],
        ];

        for expected in batches {
            enc_ctx.dedupe.as_mut().unwrap().clear();
            dec_ctx.dedupe.as_mut().unwrap().clear();
            let mut buffer = Vec::new();
            expected
                .encode_ext(&mut buffer, Some(&mut enc_ctx))
                .unwrap();
            let decoded =
                Vec::<BulkValue>::decode_ext(&mut Cursor::new(&buffer), Some(&mut dec_ctx))
                    .unwrap();
            assert_eq!(decoded, expected);
        }
    }

    #[test]
    fn test_contextual_vec_decode_multi_type_fallback() {
        let mut enc_ctx = EncoderContext {
            dedupe: Some(DedupeEncoder::new()),
            diff: None,
        };
        let mut dec_ctx = DecoderContext {
            dedupe: Some(DedupeDecoder::new()),
            diff: None,
        };
        let first = vec![BulkValue(1), BulkValue(2)];
        let second = vec![OtherBulkValue(3), OtherBulkValue(4)];
        let mut buffer = Vec::new();
        first.encode_ext(&mut buffer, Some(&mut enc_ctx)).unwrap();
        second.encode_ext(&mut buffer, Some(&mut enc_ctx)).unwrap();

        let mut cursor = Cursor::new(&buffer);
        assert_eq!(
            Vec::<BulkValue>::decode_ext(&mut cursor, Some(&mut dec_ctx)).unwrap(),
            first,
        );
        assert_eq!(
            Vec::<OtherBulkValue>::decode_ext(&mut cursor, Some(&mut dec_ctx)).unwrap(),
            second,
        );
    }

    #[test]
    fn test_multi_type_references_preserve_global_ids() {
        let mut encoder = DedupeEncoder::new();
        let mut decoder = DedupeDecoder::new();
        let mut buffer = Vec::new();

        encoder.encode::<u32, H>(&11, &mut buffer).unwrap(); // ID 1
        encoder.encode::<u64, H>(&22, &mut buffer).unwrap(); // ID 2
        encoder.encode::<u32, H>(&11, &mut buffer).unwrap(); // ID 1
        encoder.encode::<u64, H>(&22, &mut buffer).unwrap(); // ID 2
        encoder.encode::<u32, H>(&33, &mut buffer).unwrap(); // ID 3
        encoder.encode::<u64, H>(&22, &mut buffer).unwrap(); // ID 2
        encoder.encode::<u32, H>(&33, &mut buffer).unwrap(); // ID 3

        let mut cursor = Cursor::new(&buffer);
        assert_eq!(decoder.decode::<u32>(&mut cursor).unwrap(), 11);
        assert_eq!(decoder.decode::<u64>(&mut cursor).unwrap(), 22);
        assert_eq!(decoder.decode::<u32>(&mut cursor).unwrap(), 11);
        assert_eq!(decoder.decode::<u64>(&mut cursor).unwrap(), 22);
        assert_eq!(decoder.decode::<u32>(&mut cursor).unwrap(), 33);
        assert_eq!(decoder.decode::<u64>(&mut cursor).unwrap(), 22);
        assert_eq!(decoder.decode::<u32>(&mut cursor).unwrap(), 33);
    }

    #[test]
    fn test_frozen_multi_type_decoder_preserves_global_ids() {
        let mut primer_encoder = DedupeEncoder::new();
        let mut primer_decoder = DedupeDecoder::new();
        for value in [1u32, 2] {
            primer_encoder.prime::<u32, H>(&value);
            primer_decoder.prime(value);
        }
        primer_encoder.prime::<u64, H>(&7);
        primer_decoder.prime(7u64);

        let mut encoder = DedupeEncoder::with_frozen(Arc::new(primer_encoder.freeze()));
        let mut decoder = DedupeDecoder::with_frozen(Arc::new(primer_decoder.freeze()));
        let mut buffer = Vec::new();
        encoder.encode::<u32, H>(&2, &mut buffer).unwrap();
        encoder.encode::<u64, H>(&7, &mut buffer).unwrap();
        encoder.encode::<u32, H>(&1, &mut buffer).unwrap();

        let mut cursor = Cursor::new(&buffer);
        assert_eq!(decoder.decode::<u32>(&mut cursor).unwrap(), 2);
        assert_eq!(decoder.decode::<u64>(&mut cursor).unwrap(), 7);
        assert_eq!(decoder.decode::<u32>(&mut cursor).unwrap(), 1);
    }

    #[test]
    fn test_multi_type_reference_rejects_wrong_type() {
        let mut decoder = DedupeDecoder::new();
        let mut buffer = Vec::new();
        Lencode::encode_varint_u64(0, &mut buffer).unwrap();
        11u32.pack(&mut buffer).unwrap();
        Lencode::encode_varint_u64(1, &mut buffer).unwrap();

        let mut cursor = Cursor::new(&buffer);
        assert_eq!(decoder.decode::<u32>(&mut cursor).unwrap(), 11);
        assert!(matches!(
            decoder.decode::<u64>(&mut cursor),
            Err(crate::io::Error::InvalidData)
        ));
    }

    #[test]
    fn test_decode_ref_borrows_scratch_without_clone() {
        let mut buffer = Vec::new();
        Lencode::encode_varint_u64(0, &mut buffer).unwrap();
        NonCloneValue(91).pack(&mut buffer).unwrap();
        Lencode::encode_varint_u64(1, &mut buffer).unwrap();

        let mut decoder = DedupeDecoder::new();
        let mut cursor = Cursor::new(&buffer);
        let first_ptr = {
            let first = decoder.decode_ref::<NonCloneValue>(&mut cursor).unwrap();
            assert_eq!(first, &NonCloneValue(91));
            first as *const NonCloneValue
        };
        let second = decoder.decode_ref::<NonCloneValue>(&mut cursor).unwrap();
        assert_eq!(second, &NonCloneValue(91));
        assert_eq!(second as *const NonCloneValue, first_ptr);
    }

    #[test]
    fn test_decode_ref_borrows_frozen_value() {
        let mut primer = DedupeDecoder::new();
        primer.prime(BulkValue(7));
        let mut decoder = DedupeDecoder::with_frozen(Arc::new(primer.freeze()));
        let mut buffer = Vec::new();
        Lencode::encode_varint_u64(1, &mut buffer).unwrap();
        Lencode::encode_varint_u64(1, &mut buffer).unwrap();

        let mut cursor = Cursor::new(&buffer);
        let first_ptr = decoder.decode_ref::<BulkValue>(&mut cursor).unwrap() as *const BulkValue;
        let second = decoder.decode_ref::<BulkValue>(&mut cursor).unwrap();
        assert_eq!(second, &BulkValue(7));
        assert_eq!(second as *const BulkValue, first_ptr);
    }

    #[test]
    fn test_dedupe_encode_decode_roundtrip() {
        let mut encoder = DedupeEncoder::new();
        let mut decoder = DedupeDecoder::new();
        let mut buffer = Vec::new();

        // Test values
        let values = [42u32, 123u32, 42u32, 456u32, 123u32, 789u32, 42u32];

        // Encode all values
        for &value in &values {
            encoder.encode::<u32, H>(&value, &mut buffer).unwrap();
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
        encoder.encode::<u32, H>(&42u32, &mut buffer).unwrap();
        encoder.encode::<u32, H>(&123u32, &mut buffer).unwrap();

        // Clear and encode again - should start fresh
        encoder.clear();
        decoder.clear();
        buffer.clear();

        encoder.encode::<u32, H>(&42u32, &mut buffer).unwrap(); // Should be encoded as new (ID 0)
        encoder.encode::<u32, H>(&42u32, &mut buffer).unwrap(); // Should be encoded as reference (ID 1)

        let mut cursor = Cursor::new(&buffer);
        let decoded1: u32 = decoder.decode(&mut cursor).unwrap();
        let decoded2: u32 = decoder.decode(&mut cursor).unwrap();

        assert_eq!(decoded1, 42u32);
        assert_eq!(decoded2, 42u32);
    }

    #[test]
    fn test_dedupe_len_for_type() {
        let mut encoder = DedupeEncoder::new();
        let mut buffer = Vec::new();

        assert_eq!(encoder.len_for_type::<u32, H>(), 0);
        assert_eq!(encoder.num_types(), 0);

        encoder.encode::<u32, H>(&42u32, &mut buffer).unwrap();
        encoder.encode::<u32, H>(&42u32, &mut buffer).unwrap(); // duplicate, not a new entry
        encoder.encode::<u32, H>(&99u32, &mut buffer).unwrap();
        encoder.encode::<u64, H>(&7u64, &mut buffer).unwrap();

        assert_eq!(encoder.len_for_type::<u32, H>(), 2);
        assert_eq!(encoder.len_for_type::<u64, H>(), 1);
        assert_eq!(encoder.len_for_type::<u16, H>(), 0);
        assert_eq!(encoder.num_types(), 2);
        assert_eq!(encoder.len(), 3);
    }

    #[test]
    fn test_dedupe_clear_type() {
        let mut encoder = DedupeEncoder::new();
        let mut buffer = Vec::new();

        encoder.encode::<u32, H>(&42u32, &mut buffer).unwrap();
        encoder.encode::<u64, H>(&7u64, &mut buffer).unwrap();
        assert_eq!(encoder.num_types(), 2);

        encoder.clear_type::<u32>();
        assert_eq!(encoder.len_for_type::<u32, H>(), 0);
        assert_eq!(encoder.len_for_type::<u64, H>(), 1);
        assert_eq!(encoder.num_types(), 1);
    }

    #[test]
    fn test_dedupe_memory_usage() {
        let mut encoder = DedupeEncoder::new();
        let mut buffer = Vec::new();

        let initial = encoder.memory_usage();

        encoder.encode::<u32, H>(&42u32, &mut buffer).unwrap();
        encoder.encode::<u32, H>(&99u32, &mut buffer).unwrap();

        let after = encoder.memory_usage();
        assert!(
            after > initial,
            "memory usage should increase after storing entries"
        );
    }

    #[test]
    fn test_dedupe_decoder_memory_usage() {
        let decoder = DedupeDecoder::new();
        // Just verify it doesn't panic
        let _usage = decoder.memory_usage();
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

    #[test]
    fn test_prime_assigns_sequential_ids() {
        let mut encoder = DedupeEncoder::new();
        assert_eq!(encoder.prime::<u32, H>(&10), 1);
        assert_eq!(encoder.prime::<u32, H>(&20), 2);
        assert_eq!(encoder.prime::<u32, H>(&30), 3);
        assert_eq!(encoder.len(), 3);
    }

    #[test]
    fn test_prime_is_idempotent() {
        let mut encoder = DedupeEncoder::new();
        let id1 = encoder.prime::<u32, H>(&42);
        let id2 = encoder.prime::<u32, H>(&42);
        assert_eq!(id1, id2);
        assert_eq!(encoder.len(), 1);
    }

    #[test]
    fn test_prime_writes_nothing() {
        // prime() has no writer argument — this test just asserts that primed
        // values don't affect any output by confirming a subsequent encode()
        // call is the *only* thing written when the encoded value was primed.
        let mut encoder = DedupeEncoder::new();
        encoder.prime::<u32, H>(&42);
        encoder.prime::<u32, H>(&99);

        let mut buffer = Vec::new();
        encoder.encode::<u32, H>(&42, &mut buffer).unwrap();
        // Primed value → emits just the varint ID (1 byte for id=1)
        assert_eq!(buffer, vec![1]);

        buffer.clear();
        encoder.encode::<u32, H>(&99, &mut buffer).unwrap();
        assert_eq!(buffer, vec![2]);
    }

    #[test]
    fn test_prime_roundtrip_with_encode_decode() {
        let mut encoder = DedupeEncoder::new();
        let mut decoder = DedupeDecoder::new();
        let mut buffer = Vec::new();

        // Prime both sides with the same sequence
        let primed = [100u32, 200, 300, 400];
        for v in &primed {
            encoder.prime::<u32, H>(v);
            decoder.prime::<u32>(*v);
        }

        // Encode a mix of primed and novel values
        let stream = [200u32, 500, 100, 500, 400, 300, 600];
        for v in &stream {
            encoder.encode::<u32, H>(v, &mut buffer).unwrap();
        }

        // Decode and compare
        let mut cursor = Cursor::new(&buffer);
        for &expected in &stream {
            let decoded: u32 = decoder.decode(&mut cursor).unwrap();
            assert_eq!(decoded, expected);
        }
    }

    #[test]
    fn test_prime_mixed_with_encode() {
        // Calling encode() for a primed value should return just the ID
        // (no ID=0 header, no payload).
        let mut encoder = DedupeEncoder::new();
        encoder.prime::<u32, H>(&7);

        let mut buffer = Vec::new();
        let bytes = encoder.encode::<u32, H>(&7, &mut buffer).unwrap();
        // Varint for ID 1 is a single byte.
        assert_eq!(bytes, 1);
        assert_eq!(buffer, vec![1]);
    }

    #[test]
    fn test_frozen_state_basic_roundtrip() {
        // Build a primed encoder, freeze it, share via Arc across two workers,
        // and verify encode/decode round-trips work.
        let mut primer_enc = DedupeEncoder::new();
        let mut primer_dec = DedupeDecoder::new();
        let primed = [100u32, 200, 300, 400];
        for v in &primed {
            primer_enc.prime::<u32, H>(v);
            primer_dec.prime::<u32>(*v);
        }
        let frozen_enc = Arc::new(primer_enc.freeze());
        let frozen_dec = Arc::new(primer_dec.freeze());
        assert_eq!(frozen_enc.len(), 4);
        assert_eq!(frozen_dec.len(), 4);

        // Worker encoder/decoder
        let mut enc = DedupeEncoder::with_frozen(Arc::clone(&frozen_enc));
        let mut dec = DedupeDecoder::with_frozen(Arc::clone(&frozen_dec));

        // Encode mix of primed and novel values
        let stream = [200u32, 500, 100, 500, 400, 300, 600];
        let mut buffer = Vec::new();
        for v in &stream {
            enc.encode::<u32, H>(v, &mut buffer).unwrap();
        }

        // Decode
        let mut cursor = Cursor::new(&buffer);
        for &expected in &stream {
            let decoded: u32 = dec.decode(&mut cursor).unwrap();
            assert_eq!(decoded, expected);
        }
    }

    #[test]
    fn test_frozen_clear_preserves_frozen() {
        let mut primer = DedupeEncoder::new();
        primer.prime::<u32, H>(&10);
        primer.prime::<u32, H>(&20);
        let frozen = Arc::new(primer.freeze());

        let mut enc = DedupeEncoder::with_frozen(Arc::clone(&frozen));
        let mut buffer = Vec::new();

        // Encode a novel value (ID 3)
        enc.encode::<u32, H>(&99, &mut buffer).unwrap();
        assert_eq!(enc.len(), 3); // 2 frozen + 1 scratch

        // Clear; frozen state should remain
        enc.clear();
        assert_eq!(enc.len(), 2); // just the 2 frozen values
        buffer.clear();

        // Re-encode a primed value -> should emit the primed ID
        let bytes = enc.encode::<u32, H>(&10, &mut buffer).unwrap();
        assert_eq!(bytes, 1);
        assert_eq!(buffer, vec![1]); // varint(1)

        // Encode another novel value -> should get ID 3 again (next after frozen)
        buffer.clear();
        enc.encode::<u32, H>(&77, &mut buffer).unwrap();
        // First byte is varint(0) = [0]; then packed u32 (4 bytes LE for wire)
        assert_eq!(buffer[0], 0);
    }

    #[test]
    fn test_frozen_shared_across_encoders() {
        // Multiple encoders share the same frozen state. clear() on one
        // doesn't affect the others' frozen data.
        let mut primer = DedupeEncoder::new();
        primer.prime::<u32, H>(&1);
        primer.prime::<u32, H>(&2);
        primer.prime::<u32, H>(&3);
        let frozen = Arc::new(primer.freeze());

        let mut enc_a = DedupeEncoder::with_frozen(Arc::clone(&frozen));
        let mut enc_b = DedupeEncoder::with_frozen(Arc::clone(&frozen));
        let mut buf_a = Vec::new();
        let mut buf_b = Vec::new();

        enc_a.encode::<u32, H>(&1, &mut buf_a).unwrap(); // primed -> id=1
        enc_b.encode::<u32, H>(&3, &mut buf_b).unwrap(); // primed -> id=3
        assert_eq!(buf_a, vec![1]);
        assert_eq!(buf_b, vec![3]);

        // Novel encodings on each don't collide
        buf_a.clear();
        buf_b.clear();
        enc_a.encode::<u32, H>(&100, &mut buf_a).unwrap(); // id=4 on enc_a
        enc_b.encode::<u32, H>(&200, &mut buf_b).unwrap(); // id=4 on enc_b

        // Independent scratch IDs both = 4
        assert_eq!(enc_a.len(), 4);
        assert_eq!(enc_b.len(), 4);
    }

    #[test]
    #[should_panic(expected = "cannot freeze an encoder that already has a frozen state")]
    fn test_frozen_refreeze_panics() {
        let mut primer = DedupeEncoder::new();
        primer.prime::<u32, H>(&1);
        let frozen = Arc::new(primer.freeze());

        let enc = DedupeEncoder::with_frozen(frozen);
        let _ = enc.freeze(); // should panic
    }

    #[test]
    #[should_panic(expected = "cannot prime an encoder that already has a frozen state")]
    fn test_frozen_encoder_prime_panics() {
        let mut primer = DedupeEncoder::new();
        primer.prime::<u32, H>(&1);
        let frozen = Arc::new(primer.freeze());

        let mut enc = DedupeEncoder::with_frozen(frozen);
        enc.prime::<u32, H>(&99); // should panic
    }

    #[test]
    #[should_panic(expected = "cannot prime a decoder that already has a frozen state")]
    fn test_frozen_decoder_prime_panics() {
        let mut primer = DedupeDecoder::new();
        primer.prime::<u32>(1);
        let frozen = Arc::new(primer.freeze());

        let mut dec = DedupeDecoder::with_frozen(frozen);
        dec.prime::<u32>(99); // should panic
    }

    #[test]
    fn test_frozen_novel_to_scratch_then_reference() {
        // Verify novel values pushed to scratch can be re-referenced by ID.
        let mut primer_enc = DedupeEncoder::new();
        let mut primer_dec = DedupeDecoder::new();
        primer_enc.prime::<u32, H>(&10);
        primer_dec.prime::<u32>(10);
        let frozen_enc = Arc::new(primer_enc.freeze());
        let frozen_dec = Arc::new(primer_dec.freeze());

        let mut enc = DedupeEncoder::with_frozen(frozen_enc);
        let mut dec = DedupeDecoder::with_frozen(frozen_dec);
        let mut buffer = Vec::new();

        // Novel 50 (id=2) + frozen 10 (id=1) + novel 50 again (id=2)
        enc.encode::<u32, H>(&50, &mut buffer).unwrap();
        enc.encode::<u32, H>(&10, &mut buffer).unwrap();
        enc.encode::<u32, H>(&50, &mut buffer).unwrap();

        let mut cursor = Cursor::new(&buffer);
        assert_eq!(dec.decode::<u32>(&mut cursor).unwrap(), 50);
        assert_eq!(dec.decode::<u32>(&mut cursor).unwrap(), 10);
        assert_eq!(dec.decode::<u32>(&mut cursor).unwrap(), 50);
    }

    #[test]
    fn test_frozen_empty_encoder() {
        // Freezing an un-primed encoder yields a valid empty frozen state.
        let enc = DedupeEncoder::new();
        let frozen = Arc::new(enc.freeze());
        assert_eq!(frozen.len(), 0);
        assert!(frozen.is_empty());

        // Worker backed by an empty frozen state behaves like a fresh encoder.
        let mut worker = DedupeEncoder::with_frozen(frozen);
        assert_eq!(worker.len(), 0);
        let mut buffer = Vec::new();
        worker.encode::<u32, H>(&42, &mut buffer).unwrap();
        assert_eq!(worker.len(), 1);
    }

    #[test]
    fn test_frozen_multi_type_encoder() {
        // Encoder side supports multiple types in the frozen state.
        let mut primer = DedupeEncoder::new();
        primer.prime::<u32, H>(&1);
        primer.prime::<u64, H>(&7);
        primer.prime::<u32, H>(&2);
        let frozen = Arc::new(primer.freeze());
        assert_eq!(frozen.len(), 3);

        let mut enc = DedupeEncoder::with_frozen(frozen);
        let mut buffer = Vec::new();

        // u32:1 is primed with ID 1
        enc.encode::<u32, H>(&1, &mut buffer).unwrap();
        assert_eq!(buffer, vec![1]);

        // u64:7 is primed with ID 2
        buffer.clear();
        enc.encode::<u64, H>(&7, &mut buffer).unwrap();
        assert_eq!(buffer, vec![2]);

        // u32:2 is primed with ID 3
        buffer.clear();
        enc.encode::<u32, H>(&2, &mut buffer).unwrap();
        assert_eq!(buffer, vec![3]);
    }

    #[test]
    fn test_frozen_encoder_memory_usage_stable_after_clear() {
        // clear() shouldn't drop the scratch allocation entirely; memory usage
        // should remain bounded and stable.
        let mut primer = DedupeEncoder::new();
        for i in 0..100u32 {
            primer.prime::<u32, H>(&i);
        }
        let frozen = Arc::new(primer.freeze());

        let mut enc = DedupeEncoder::with_frozen(frozen);
        let mut buffer = Vec::new();

        // Encode some novel values
        for i in 1000..1100u32 {
            enc.encode::<u32, H>(&i, &mut buffer).unwrap();
        }
        let post_encode = enc.len();
        assert_eq!(post_encode, 200); // 100 frozen + 100 novel

        // After clear, only frozen remains
        enc.clear();
        assert_eq!(enc.len(), 100);
    }

    // Compile-time assertions: FrozenEncoderState / FrozenDecoderState must be
    // Send + Sync so they can be wrapped in Arc and shared across threads.
    const _: fn() = || {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<FrozenEncoderState>();
        assert_send_sync::<FrozenDecoderState>();
    };

    #[cfg(feature = "std")]
    #[test]
    fn test_frozen_shared_across_threads() {
        // Confirms Arc<FrozenEncoderState> can be shared across threads and
        // each worker encodes independently.
        use std::thread;

        let mut primer_enc = DedupeEncoder::new();
        let mut primer_dec = DedupeDecoder::new();
        for i in 0..50u32 {
            primer_enc.prime::<u32, H>(&i);
            primer_dec.prime::<u32>(i);
        }
        let frozen_enc = Arc::new(primer_enc.freeze());
        let frozen_dec = Arc::new(primer_dec.freeze());

        // Spawn 4 workers; each encodes its own stream and verifies it decodes.
        let handles: Vec<_> = (0..4)
            .map(|worker_id| {
                let frozen_enc = Arc::clone(&frozen_enc);
                let frozen_dec = Arc::clone(&frozen_dec);
                thread::spawn(move || {
                    let mut enc = DedupeEncoder::with_frozen(frozen_enc);
                    let mut dec = DedupeDecoder::with_frozen(frozen_dec);
                    let mut buffer = Vec::new();

                    // Mix of primed and novel values, unique per worker
                    let stream: Vec<u32> = (0..25u32)
                        .chain(core::iter::once(1000 + worker_id))
                        .collect();
                    for v in &stream {
                        enc.encode::<u32, H>(v, &mut buffer).unwrap();
                    }
                    let mut cursor = Cursor::new(&buffer);
                    for &expected in &stream {
                        let decoded: u32 = dec.decode(&mut cursor).unwrap();
                        assert_eq!(decoded, expected, "worker {worker_id}");
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    fn test_clear_preserves_scratch_across_multiple_cycles() {
        // Simulates multiple slot boundaries: clear between cycles, verify
        // each cycle works independently and IDs don't collide.
        let mut primer_enc = DedupeEncoder::new();
        let mut primer_dec = DedupeDecoder::new();
        primer_enc.prime::<u32, H>(&10);
        primer_enc.prime::<u32, H>(&20);
        primer_dec.prime::<u32>(10);
        primer_dec.prime::<u32>(20);
        let frozen_enc = Arc::new(primer_enc.freeze());
        let frozen_dec = Arc::new(primer_dec.freeze());

        let mut enc = DedupeEncoder::with_frozen(frozen_enc);
        let mut dec = DedupeDecoder::with_frozen(frozen_dec);

        for cycle in 0..5u32 {
            enc.clear();
            dec.clear();

            let novel_a = 100 + cycle;
            let novel_b = 200 + cycle;

            let mut buffer = Vec::new();
            enc.encode::<u32, H>(&10, &mut buffer).unwrap(); // primed id=1
            enc.encode::<u32, H>(&novel_a, &mut buffer).unwrap(); // novel id=3
            enc.encode::<u32, H>(&20, &mut buffer).unwrap(); // primed id=2
            enc.encode::<u32, H>(&novel_b, &mut buffer).unwrap(); // novel id=4
            enc.encode::<u32, H>(&novel_a, &mut buffer).unwrap(); // ref id=3

            assert_eq!(enc.len(), 4); // 2 frozen + 2 scratch

            let mut cursor = Cursor::new(&buffer);
            assert_eq!(dec.decode::<u32>(&mut cursor).unwrap(), 10);
            assert_eq!(dec.decode::<u32>(&mut cursor).unwrap(), novel_a);
            assert_eq!(dec.decode::<u32>(&mut cursor).unwrap(), 20);
            assert_eq!(dec.decode::<u32>(&mut cursor).unwrap(), novel_b);
            assert_eq!(dec.decode::<u32>(&mut cursor).unwrap(), novel_a);
        }
    }

    #[test]
    fn test_frozen_large_primed_set() {
        // Simulates the horizon use case: a large primed set (1000 pubkey-like
        // values), encoded/decoded with a mix of primed and novel entries.
        let mut primer_enc = DedupeEncoder::new();
        let mut primer_dec = DedupeDecoder::new();
        for i in 0..1000u32 {
            primer_enc.prime::<u32, H>(&i);
            primer_dec.prime::<u32>(i);
        }
        let frozen_enc = Arc::new(primer_enc.freeze());
        let frozen_dec = Arc::new(primer_dec.freeze());
        assert_eq!(frozen_enc.len(), 1000);

        let mut enc = DedupeEncoder::with_frozen(frozen_enc);
        let mut dec = DedupeDecoder::with_frozen(frozen_dec);
        let mut buffer = Vec::new();

        // Encode: 500 primed hits + 500 novel values
        let stream: Vec<u32> = (0..500).chain(10_000..10_500).collect();
        for v in &stream {
            enc.encode::<u32, H>(v, &mut buffer).unwrap();
        }

        // Decode and compare
        let mut cursor = Cursor::new(&buffer);
        for &expected in &stream {
            let decoded: u32 = dec.decode(&mut cursor).unwrap();
            assert_eq!(decoded, expected);
        }
    }

    #[test]
    fn test_prime_survives_novel_values() {
        // A novel encoded value should be inserted after primed IDs without
        // disturbing them.
        let mut encoder = DedupeEncoder::new();
        let mut decoder = DedupeDecoder::new();
        let mut buffer = Vec::new();

        encoder.prime::<u32, H>(&10);
        encoder.prime::<u32, H>(&20);
        decoder.prime::<u32>(10);
        decoder.prime::<u32>(20);

        // Encode novel value 30 (should get ID 3)
        encoder.encode::<u32, H>(&30, &mut buffer).unwrap();
        // Re-encode 10 (primed, should emit ID 1)
        encoder.encode::<u32, H>(&10, &mut buffer).unwrap();
        // Re-encode 30 (now seen, should emit ID 3)
        encoder.encode::<u32, H>(&30, &mut buffer).unwrap();

        let mut cursor = Cursor::new(&buffer);
        assert_eq!(decoder.decode::<u32>(&mut cursor).unwrap(), 30);
        assert_eq!(decoder.decode::<u32>(&mut cursor).unwrap(), 10);
        assert_eq!(decoder.decode::<u32>(&mut cursor).unwrap(), 30);
    }
}
