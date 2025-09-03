use solana_sdk::{
    hash::{HASH_BYTES, Hash},
    message::MessageHeader,
    pubkey::Pubkey,
    signature::{SIGNATURE_BYTES, Signature},
    transaction::SanitizedTransaction,
};

use crate::prelude::*;

impl Pack for Pubkey {
    #[inline(always)]
    fn pack(&self, writer: &mut impl Write) -> Result<usize> {
        self.as_array().pack(writer)
    }

    #[inline(always)]
    fn unpack(reader: &mut impl Read) -> Result<Self> {
        let mut buf = [0u8; 32];
        if reader.read(&mut buf)? != 32 {
            return Err(Error::ReaderOutOfData);
        }
        Ok(Pubkey::new_from_array(buf))
    }
}

// Pubkeys are commonly repeated in Solana transactions, so we use deduplication
// to avoid encoding the same pubkey multiple times
impl Encode for Pubkey {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        if let Some(dedupe_encoder) = dedupe_encoder {
            dedupe_encoder.encode(self, writer)
        } else {
            // Pubkeys require deduplication - that's their main benefit
            Err(Error::InvalidData)
        }
    }
}

impl Decode for Pubkey {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        if let Some(dedupe_decoder) = dedupe_decoder {
            dedupe_decoder.decode(reader)
        } else {
            // Pubkeys require deduplication - that's their main benefit
            Err(Error::InvalidData)
        }
    }
}

impl Encode for Hash {
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        self.to_bytes().encode_ext(writer, dedupe_encoder)
    }
}

impl Decode for Hash {
    fn decode_ext(
        reader: &mut impl Read,
        dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        let bytes = <[u8; HASH_BYTES]>::decode_ext(reader, dedupe_decoder)?;
        Ok(Hash::new_from_array(bytes))
    }
}

impl Encode for Signature {
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        self.as_array().encode_ext(writer, dedupe_encoder)
    }
}

impl Decode for Signature {
    fn decode_ext(
        reader: &mut impl Read,
        _dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        let sig: [u8; SIGNATURE_BYTES] = decode(reader)?;
        Ok(Signature::from(sig))
    }
}

impl Encode for MessageHeader {
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        let combined = u32::from_le_bytes([
            self.num_required_signatures,
            self.num_readonly_signed_accounts,
            self.num_readonly_unsigned_accounts,
            0,
        ]);
        combined.encode_ext(writer, dedupe_encoder)
    }
}

impl Decode for MessageHeader {
    fn decode_ext(
        reader: &mut impl Read,
        _dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        let combined: u32 = decode(reader)?;
        let combined_bytes = combined.to_le_bytes();
        Ok(MessageHeader {
            num_required_signatures: combined_bytes[0],
            num_readonly_signed_accounts: combined_bytes[1],
            num_readonly_unsigned_accounts: combined_bytes[2],
        })
    }
}

impl Encode for SanitizedTransaction {
    #[inline(always)]
    fn encode_ext(
        &self,
        _writer: &mut impl Write,
        _dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        todo!()
    }
}

#[test]
fn test_encode_decode_pubkey() {
    use crate::dedupe::{DedupeDecoder, DedupeEncoder};

    // Create shared deduper instances that persist across the loop
    let mut buf = Vec::new();
    let mut dedupe_encoder = DedupeEncoder::new();
    let mut encoded_pubkeys = Vec::<Pubkey>::new();

    // Encode some pubkeys, including duplicates to test deduplication
    for i in 0..10 {
        let pubkey = if i < 5 {
            Pubkey::new_unique()
        } else {
            // Reuse some pubkeys to test deduplication
            encoded_pubkeys[i - 5].clone()
        };

        let bytes_before = buf.len();
        pubkey
            .encode_ext(&mut buf, Some(&mut dedupe_encoder))
            .unwrap();
        let bytes_written = buf.len() - bytes_before;

        if i < 5 {
            // First time seeing each pubkey: 1 byte (id=0) + 32 bytes (data) = 33 bytes
            assert_eq!(bytes_written, 33);
            encoded_pubkeys.push(pubkey);
        } else {
            // Duplicate pubkey: just the ID, should be 1 byte
            assert_eq!(bytes_written, 1);
        }
    }

    // Decode all pubkeys
    let mut cursor = Cursor::new(&buf);
    let mut dedupe_decoder = DedupeDecoder::new();
    let mut decoded_pubkeys = Vec::new();

    for _ in 0..10 {
        let decoded_pubkey = Pubkey::decode_ext(&mut cursor, Some(&mut dedupe_decoder)).unwrap();
        decoded_pubkeys.push(decoded_pubkey);
    }

    // Verify the pattern: first 5 unique, then 5 duplicates
    for i in 0..10 {
        if i < 5 {
            assert_eq!(decoded_pubkeys[i], encoded_pubkeys[i]);
        } else {
            assert_eq!(decoded_pubkeys[i], encoded_pubkeys[i - 5]);
        }
    }
}

#[test]
fn test_encode_decode_message_header() {
    for _ in 0..1000 {
        let header = MessageHeader {
            num_required_signatures: rand::random::<u8>(),
            num_readonly_signed_accounts: rand::random::<u8>(),
            num_readonly_unsigned_accounts: rand::random::<u8>(),
        };
        let mut buf = [0u8; 4];
        let mut cursor = Cursor::new(&mut buf);
        header.encode(&mut cursor).unwrap();
        let decoded_header = MessageHeader::decode(&mut Cursor::new(&mut buf)).unwrap();
        assert_eq!(header, decoded_header);
    }
    let header = MessageHeader {
        num_required_signatures: 0,
        num_readonly_signed_accounts: 0,
        num_readonly_unsigned_accounts: 0,
    };
    let mut buf = [0u8; 4];
    let mut cursor = Cursor::new(&mut buf);
    let n = header.encode(&mut cursor).unwrap();
    assert_eq!(n, 1);
    let decoded_header = MessageHeader::decode(&mut Cursor::new(&mut buf)).unwrap();
    assert_eq!(header, decoded_header);
}

#[test]
fn test_pubkey_pack_roundtrip() {
    for _ in 0..1000 {
        let pubkey = Pubkey::new_unique();
        let mut buf = [0u8; 32];
        let mut cursor = Cursor::new(&mut buf);
        let n = pubkey.pack(&mut cursor).unwrap();
        assert_eq!(n, 32);
        let unpacked_pubkey = Pubkey::unpack(&mut Cursor::new(&mut buf)).unwrap();
        assert_eq!(pubkey, unpacked_pubkey);
    }
}

#[test]
fn test_pubkey_deduplication() {
    use crate::dedupe::{DedupeDecoder, DedupeEncoder};

    // Create some test pubkeys, with duplicates
    let pubkey1 = Pubkey::new_unique();
    let pubkey2 = Pubkey::new_unique();
    let pubkey3 = pubkey1; // Duplicate of pubkey1
    let pubkeys = vec![pubkey1, pubkey2, pubkey3, pubkey1, pubkey2]; // More duplicates

    // Encode with deduplication
    let mut buf = Vec::new();
    let mut dedupe_encoder = DedupeEncoder::new();

    let mut total_bytes = 0;
    for pubkey in &pubkeys {
        total_bytes += pubkey
            .encode_ext(&mut buf, Some(&mut dedupe_encoder))
            .unwrap();
    }

    // With deduplication, we should save space by not repeating pubkeys
    // First pubkey: 1 byte (id=0) + 32 bytes (data) = 33 bytes
    // Second pubkey: 1 byte (id=0) + 32 bytes (data) = 33 bytes
    // Third pubkey (duplicate of first): 1 byte (id=1) = 1 byte
    // Fourth pubkey (duplicate of first): 1 byte (id=1) = 1 byte
    // Fifth pubkey (duplicate of second): 1 byte (id=2) = 1 byte
    // Total: 33 + 33 + 1 + 1 + 1 = 69 bytes
    assert_eq!(total_bytes, 69);

    // Decode with deduplication
    let mut decode_cursor = Cursor::new(&buf);
    let mut dedupe_decoder = DedupeDecoder::new();
    let mut decoded_pubkeys = Vec::new();

    for _ in 0..pubkeys.len() {
        decoded_pubkeys
            .push(Pubkey::decode_ext(&mut decode_cursor, Some(&mut dedupe_decoder)).unwrap());
    }

    // Verify all pubkeys were decoded correctly
    assert_eq!(decoded_pubkeys, pubkeys);

    // Verify deduplication worked - should have only 2 unique pubkeys stored
    assert_eq!(dedupe_decoder.len(), 2);
}

#[test]
fn test_pubkey_deduplication_without_duplicates() {
    use crate::dedupe::{DedupeDecoder, DedupeEncoder};

    // Create unique pubkeys
    let pubkeys: Vec<Pubkey> = (0..5).map(|_| Pubkey::new_unique()).collect();

    // Encode with deduplication
    let mut buf = Vec::new();
    let mut dedupe_encoder = DedupeEncoder::new();

    let mut total_bytes = 0;
    for pubkey in &pubkeys {
        total_bytes += pubkey
            .encode_ext(&mut buf, Some(&mut dedupe_encoder))
            .unwrap();
    }

    // Without duplicates, each pubkey should take 33 bytes (1 + 32)
    assert_eq!(total_bytes, 5 * 33);

    // Decode and verify
    let mut decode_cursor = Cursor::new(&buf);
    let mut dedupe_decoder = DedupeDecoder::new();
    let mut decoded_pubkeys = Vec::new();

    for _ in 0..pubkeys.len() {
        decoded_pubkeys
            .push(Pubkey::decode_ext(&mut decode_cursor, Some(&mut dedupe_decoder)).unwrap());
    }

    assert_eq!(decoded_pubkeys, pubkeys);
    assert_eq!(dedupe_decoder.len(), 5);
}

#[test]
fn test_pubkey_requires_deduplication() {
    let pubkey = Pubkey::new_unique();
    let mut buf = Vec::new();

    // Should fail when trying to encode without deduplication
    assert!(pubkey.encode_ext(&mut buf, None).is_err());

    // Should fail when trying to decode without deduplication
    let mut cursor = Cursor::new(&buf);
    assert!(Pubkey::decode_ext(&mut cursor, None).is_err());
}
