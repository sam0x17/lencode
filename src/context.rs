//! Unified encoding/decoding context that bundles optional deduplication and diff state.

use crate::dedupe::{DedupeDecoder, DedupeEncoder};
use crate::diff::{DiffDecoder, DiffEncoder};

/// Bundles optional [`DedupeEncoder`] and [`DiffEncoder`] state for encoding.
///
/// Pass `Some(&mut EncoderContext)` to [`Encode::encode_ext`] when you want
/// deduplication, diff encoding, or both. Individual components are optional:
/// leave a field `None` to disable that feature.
pub struct EncoderContext {
    /// Optional deduplication encoder.
    pub dedupe: Option<DedupeEncoder>,
    /// Optional diff encoder for byte blobs.
    pub diff: Option<DiffEncoder>,
}

impl Default for EncoderContext {
    fn default() -> Self {
        Self::new()
    }
}

impl EncoderContext {
    /// Creates a new context with no features enabled.
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            dedupe: None,
            diff: None,
        }
    }

    /// Creates a context with deduplication enabled.
    #[inline(always)]
    pub fn with_dedupe() -> Self {
        Self {
            dedupe: Some(DedupeEncoder::new()),
            diff: None,
        }
    }

    /// Creates a context with diff encoding enabled.
    #[inline(always)]
    pub fn with_diff() -> Self {
        Self {
            dedupe: None,
            diff: Some(DiffEncoder::new()),
        }
    }

    /// Creates a context with both deduplication and diff encoding enabled.
    #[inline(always)]
    pub fn with_all() -> Self {
        Self {
            dedupe: Some(DedupeEncoder::new()),
            diff: Some(DiffEncoder::new()),
        }
    }
}

/// Bundles optional [`DedupeDecoder`] and [`DiffDecoder`] state for decoding.
///
/// Pass `Some(&mut DecoderContext)` to [`Decode::decode_ext`] when you want
/// deduplication, diff decoding, or both.
pub struct DecoderContext {
    /// Optional deduplication decoder.
    pub dedupe: Option<DedupeDecoder>,
    /// Optional diff decoder for byte blobs.
    pub diff: Option<DiffDecoder>,
}

impl Default for DecoderContext {
    fn default() -> Self {
        Self::new()
    }
}

impl DecoderContext {
    /// Creates a new context with no features enabled.
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            dedupe: None,
            diff: None,
        }
    }

    /// Creates a context with deduplication enabled.
    #[inline(always)]
    pub fn with_dedupe() -> Self {
        Self {
            dedupe: Some(DedupeDecoder::new()),
            diff: None,
        }
    }

    /// Creates a context with diff decoding enabled.
    #[inline(always)]
    pub fn with_diff() -> Self {
        Self {
            dedupe: None,
            diff: Some(DiffDecoder::new()),
        }
    }

    /// Creates a context with both deduplication and diff decoding enabled.
    #[inline(always)]
    pub fn with_all() -> Self {
        Self {
            dedupe: Some(DedupeDecoder::new()),
            diff: Some(DiffDecoder::new()),
        }
    }
}
