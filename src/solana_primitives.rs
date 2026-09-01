//! Encoding support for official lightweight Solana primitives.
//!
//! This deliberately small feature lets applications use current lencode
//! core improvements without pulling in the complete reference-transaction
//! dependency graph. The full `solana-types` feature remains available for
//! lencode's reference transaction codecs.

use solana_address::Address;

use crate::prelude::*;

impl Pack for Address {
    #[inline(always)]
    fn pack(&self, writer: &mut impl Write) -> Result<usize> {
        self.to_bytes().pack(writer)
    }

    #[inline(always)]
    fn unpack(reader: &mut impl Read) -> Result<Self> {
        let mut bytes = [0u8; 32];
        crate::io::read_exact_bytes(reader, &mut bytes)?;
        Ok(Self::new_from_array(bytes))
    }
}

// Account and transaction addresses are attacker-controlled, so use the
// keyed full-value hasher rather than a fast partial-key hasher.
impl DedupeEncodeable for Address {
    type Hasher = crate::dedupe::DefaultDedupeHasher;
}

impl DedupeDecodeable for Address {
    type Hasher = crate::dedupe::DefaultDedupeHasher;
}

// The full reference-type module provides these same implementations when
// enabled. Keep feature sets additive by compiling the primitive-only copies
// only when that module is absent.
#[cfg(not(feature = "solana-types"))]
impl Encode for solana_hash::Hash {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        ctx: Option<&mut EncoderContext>,
    ) -> Result<usize> {
        self.as_bytes().encode_ext(writer, ctx)
    }
}

#[cfg(not(feature = "solana-types"))]
impl Decode for solana_hash::Hash {
    #[inline(always)]
    fn decode_ext(reader: &mut impl Read, ctx: Option<&mut DecoderContext>) -> Result<Self> {
        let bytes = <[u8; solana_hash::HASH_BYTES]>::decode_ext(reader, ctx)?;
        Ok(Self::new_from_array(bytes))
    }
}

#[cfg(not(feature = "solana-types"))]
impl Encode for solana_signature::Signature {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        ctx: Option<&mut EncoderContext>,
    ) -> Result<usize> {
        self.as_array().encode_ext(writer, ctx)
    }
}

#[cfg(not(feature = "solana-types"))]
impl Decode for solana_signature::Signature {
    #[inline(always)]
    fn decode_ext(reader: &mut impl Read, _ctx: Option<&mut DecoderContext>) -> Result<Self> {
        let bytes: [u8; solana_signature::SIGNATURE_BYTES] = decode(reader)?;
        Ok(Self::from(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dedupe::{DedupeDecoder, DedupeEncoder};
    use crate::io::Cursor;

    struct ChunkedReader<'a> {
        bytes: &'a [u8],
        position: usize,
    }

    impl Read for ChunkedReader<'_> {
        fn read(&mut self, output: &mut [u8]) -> Result<usize> {
            if self.position == self.bytes.len() {
                return Ok(0);
            }
            let len = output.len().min(3).min(self.bytes.len() - self.position);
            output[..len].copy_from_slice(&self.bytes[self.position..self.position + len]);
            self.position += len;
            Ok(len)
        }
    }

    #[test]
    fn primitive_roundtrip_and_address_dedupe() {
        let address = Address::from([7u8; 32]);
        let hash = solana_hash::Hash::new_from_array([8u8; 32]);
        let signature = solana_signature::Signature::from([9u8; 64]);

        let mut encoder = EncoderContext {
            dedupe: Some(DedupeEncoder::new()),
            diff: None,
        };
        let mut bytes = Vec::new();
        address.encode_ext(&mut bytes, Some(&mut encoder)).unwrap();
        address.encode_ext(&mut bytes, Some(&mut encoder)).unwrap();
        hash.encode_ext(&mut bytes, None).unwrap();
        signature.encode_ext(&mut bytes, None).unwrap();

        let mut decoder = DecoderContext {
            dedupe: Some(DedupeDecoder::new()),
            diff: None,
        };
        let mut cursor = Cursor::new(bytes.as_slice());
        assert_eq!(
            Address::decode_ext(&mut cursor, Some(&mut decoder)).unwrap(),
            address
        );
        assert_eq!(
            Address::decode_ext(&mut cursor, Some(&mut decoder)).unwrap(),
            address
        );
        assert_eq!(
            solana_hash::Hash::decode_ext(&mut cursor, None).unwrap(),
            hash
        );
        assert_eq!(
            solana_signature::Signature::decode_ext(&mut cursor, None).unwrap(),
            signature
        );
    }

    #[test]
    fn address_unpack_accepts_short_reads_and_rejects_truncation() {
        let bytes = [0xabu8; 32];
        let mut chunked = ChunkedReader {
            bytes: &bytes,
            position: 0,
        };
        assert_eq!(Address::unpack(&mut chunked).unwrap().to_bytes(), bytes);

        let mut truncated = ChunkedReader {
            bytes: &bytes[..31],
            position: 0,
        };
        assert!(matches!(
            Address::unpack(&mut truncated),
            Err(Error::ReaderOutOfData)
        ));
    }
}
