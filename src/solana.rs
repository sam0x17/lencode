use solana_sdk::{
    hash::{HASH_BYTES, Hash},
    instruction::CompiledInstruction,
    message::{
        LegacyMessage, Message, MessageHeader, SanitizedMessage, VersionedMessage,
        v0::{self, MessageAddressTableLookup},
    },
    pubkey::Pubkey,
    signature::{SIGNATURE_BYTES, Signature},
    transaction::{SanitizedTransaction, VersionedTransaction},
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

impl DedupeEncodeable for Pubkey {}
impl DedupeDecodeable for Pubkey {}

impl Encode for Hash {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        self.to_bytes().encode_ext(writer, dedupe_encoder)
    }
}

impl Decode for Hash {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let bytes = <[u8; HASH_BYTES]>::decode_ext(reader, dedupe_decoder)?;
        Ok(Hash::new_from_array(bytes))
    }
}

impl Encode for Signature {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        self.as_array().encode_ext(writer, dedupe_encoder)
    }
}

impl Decode for Signature {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        _dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let sig: [u8; SIGNATURE_BYTES] = decode(reader)?;
        Ok(Signature::from(sig))
    }
}

impl Encode for MessageHeader {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut DedupeEncoder>,
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
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        _dedupe_decoder: Option<&mut DedupeDecoder>,
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

impl Encode for CompiledInstruction {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_bytes = 0;
        total_bytes += self
            .program_id_index
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_bytes += self
            .accounts
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_bytes += self.data.encode_ext(writer, dedupe_encoder)?;
        Ok(total_bytes)
    }
}

impl Decode for CompiledInstruction {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let program_id_index: u8 = u8::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let accounts: Vec<u8> = Vec::<u8>::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let data: Vec<u8> = Vec::<u8>::decode_ext(reader, dedupe_decoder)?;
        Ok(CompiledInstruction {
            program_id_index,
            accounts,
            data,
        })
    }
}

impl Encode for Message {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_bytes = 0;
        total_bytes += self
            .header
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_bytes += self
            .account_keys
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_bytes += self
            .recent_blockhash
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_bytes += self.instructions.encode_ext(writer, dedupe_encoder)?;
        Ok(total_bytes)
    }
}

impl Decode for Message {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let header: MessageHeader =
            MessageHeader::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let account_keys: Vec<Pubkey> =
            Vec::<Pubkey>::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let recent_blockhash: Hash = Hash::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let instructions: Vec<CompiledInstruction> =
            Vec::<CompiledInstruction>::decode_ext(reader, dedupe_decoder)?;
        Ok(Message {
            header,
            account_keys,
            recent_blockhash,
            instructions,
        })
    }
}

impl Encode for MessageAddressTableLookup {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_bytes = 0;
        total_bytes += self
            .account_key
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_bytes += self
            .writable_indexes
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_bytes += self.readonly_indexes.encode_ext(writer, dedupe_encoder)?;
        Ok(total_bytes)
    }
}

impl Decode for MessageAddressTableLookup {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let account_key: Pubkey = Pubkey::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let writable_indexes: Vec<u8> =
            Vec::<u8>::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let readonly_indexes: Vec<u8> = Vec::<u8>::decode_ext(reader, dedupe_decoder)?;
        Ok(MessageAddressTableLookup {
            account_key,
            writable_indexes,
            readonly_indexes,
        })
    }
}

impl Encode for v0::Message {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_bytes = 0;
        total_bytes += self
            .header
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_bytes += self
            .account_keys
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_bytes += self
            .recent_blockhash
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_bytes += self
            .instructions
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_bytes += self
            .address_table_lookups
            .encode_ext(writer, dedupe_encoder)?;
        Ok(total_bytes)
    }
}

impl Decode for v0::Message {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let header: MessageHeader =
            MessageHeader::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let account_keys: Vec<Pubkey> =
            Vec::<Pubkey>::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let recent_blockhash: Hash = Hash::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let instructions: Vec<CompiledInstruction> =
            Vec::<CompiledInstruction>::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let address_table_lookups: Vec<MessageAddressTableLookup> =
            Vec::<MessageAddressTableLookup>::decode_ext(reader, dedupe_decoder)?;
        Ok(v0::Message {
            header,
            account_keys,
            recent_blockhash,
            instructions,
            address_table_lookups,
        })
    }
}

impl Encode for LegacyMessage<'_> {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_bytes = 0;
        total_bytes += self
            .message
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_bytes += self
            .is_writable_account_cache
            .encode_ext(writer, dedupe_encoder)?;
        Ok(total_bytes)
    }
}

impl Decode for LegacyMessage<'_> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let message: Message = Message::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let is_writable_account_cache: Vec<bool> = Vec::<bool>::decode_ext(reader, dedupe_decoder)?;
        Ok(LegacyMessage {
            message: std::borrow::Cow::Owned(message),
            is_writable_account_cache,
        })
    }
}

impl Encode for SanitizedMessage {
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_bytes = 0;
        match self {
            SanitizedMessage::Legacy(inner) => {
                total_bytes += <usize as Encode>::encode_discriminant(0, writer)?;
                total_bytes += inner.encode_ext(writer, dedupe_encoder)?;
            }
            SanitizedMessage::V0(inner) => {
                total_bytes += <usize as Encode>::encode_discriminant(1, writer)?;
                total_bytes += inner.encode_ext(writer, dedupe_encoder)?;
            }
        }
        Ok(total_bytes)
    }
}

impl Encode for v0::LoadedAddresses {
    #[inline(always)]
    #[allow(clippy::needless_option_as_deref)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut total = 0;
        total += self
            .writable
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total += self
            .readonly
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        Ok(total)
    }
}

impl Decode for v0::LoadedAddresses {
    #[inline(always)]
    #[allow(clippy::needless_option_as_deref)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let writable = Vec::<Pubkey>::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let readonly = Vec::<Pubkey>::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        Ok(v0::LoadedAddresses { writable, readonly })
    }
}

impl<'a> Encode for v0::LoadedMessage<'a> {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut total = 0;
        total += self
            .message
            .as_ref()
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total += self
            .loaded_addresses
            .as_ref()
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total += self
            .is_writable_account_cache
            .encode_ext(writer, dedupe_encoder)?;
        Ok(total)
    }
}

impl<'a> Decode for v0::LoadedMessage<'a> {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let message = v0::Message::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let loaded_addresses =
            v0::LoadedAddresses::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let is_writable_account_cache = Vec::<bool>::decode_ext(reader, dedupe_decoder)?;
        Ok(v0::LoadedMessage {
            message: std::borrow::Cow::Owned(message),
            loaded_addresses: std::borrow::Cow::Owned(loaded_addresses),
            is_writable_account_cache,
        })
    }
}

impl Encode for SanitizedTransaction {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut total = 0;
        total += self
            .message()
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total += self
            .message_hash()
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total += self
            .is_simple_vote_transaction()
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        let sigs: Vec<Signature> = self.signatures().to_vec();
        total += sigs.encode_ext(writer, dedupe_encoder)?;
        Ok(total)
    }
}

impl Decode for SanitizedTransaction {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let message = SanitizedMessage::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let message_hash = Hash::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let is_simple_vote_tx = bool::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let signatures = Vec::<Signature>::decode_ext(reader, dedupe_decoder)?;
        SanitizedTransaction::try_new_from_fields(
            message,
            message_hash,
            is_simple_vote_tx,
            signatures,
        )
        .map_err(|_| Error::InvalidData)
    }
}

impl Decode for SanitizedMessage {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let disc = <usize as Decode>::decode_discriminant(reader)?;
        match disc {
            0 => {
                let inner = LegacyMessage::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
                Ok(SanitizedMessage::Legacy(inner))
            }
            1 => {
                let inner = v0::LoadedMessage::decode_ext(reader, dedupe_decoder)?;
                Ok(SanitizedMessage::V0(inner))
            }
            _ => Err(Error::InvalidData),
        }
    }
}

impl Encode for VersionedMessage {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut total = 0;
        match self {
            VersionedMessage::Legacy(message) => {
                total += <usize as Encode>::encode_discriminant(0, writer)?;
                total += message.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
            }
            VersionedMessage::V0(message) => {
                total += <usize as Encode>::encode_discriminant(1, writer)?;
                total += message.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
            }
        }
        Ok(total)
    }
}

impl Decode for VersionedMessage {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let disc = <usize as Decode>::decode_discriminant(reader)?;
        match disc {
            0 => {
                let legacy = Message::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
                Ok(VersionedMessage::Legacy(legacy))
            }
            1 => {
                let v0msg = v0::Message::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
                Ok(VersionedMessage::V0(v0msg))
            }
            _ => Err(Error::InvalidData),
        }
    }
}

impl Encode for VersionedTransaction {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut total = 0;
        total += self
            .signatures
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total += self
            .message
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        Ok(total)
    }
}

impl Decode for VersionedTransaction {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let signatures = Vec::<Signature>::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let message = VersionedMessage::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        Ok(VersionedTransaction {
            signatures,
            message,
        })
    }
}

#[test]
fn test_versioned_message_encode_decode_legacy() {
    let header = MessageHeader {
        num_required_signatures: 1,
        num_readonly_signed_accounts: 0,
        num_readonly_unsigned_accounts: 1,
    };
    let account_keys = vec![Pubkey::new_unique(), Pubkey::new_unique()];
    let recent_blockhash = Hash::new_unique();
    let instructions = vec![CompiledInstruction {
        program_id_index: 0,
        accounts: vec![1],
        data: vec![1, 2, 3],
    }];
    let legacy = Message {
        header,
        account_keys,
        recent_blockhash,
        instructions,
    };
    let vm = VersionedMessage::Legacy(legacy);

    let mut buf = Vec::new();
    vm.encode(&mut buf).unwrap();
    let decoded = VersionedMessage::decode(&mut std::io::Cursor::new(&buf)).unwrap();
    assert_eq!(vm, decoded);
}

#[test]
fn test_versioned_message_encode_decode_v0() {
    let header = MessageHeader {
        num_required_signatures: 1,
        num_readonly_signed_accounts: 0,
        num_readonly_unsigned_accounts: 1,
    };
    let account_keys = vec![Pubkey::new_unique(), Pubkey::new_unique()];
    let recent_blockhash = Hash::new_unique();
    let instructions = vec![CompiledInstruction {
        program_id_index: 1,
        accounts: vec![0],
        data: vec![9, 9],
    }];
    let address_table_lookups: Vec<MessageAddressTableLookup> = Vec::new();
    let v0msg = v0::Message {
        header,
        account_keys,
        recent_blockhash,
        instructions,
        address_table_lookups,
    };
    let vm = VersionedMessage::V0(v0msg);

    let mut buf = Vec::new();
    vm.encode(&mut buf).unwrap();
    let decoded = VersionedMessage::decode(&mut std::io::Cursor::new(&buf)).unwrap();
    assert_eq!(vm, decoded);
}

#[test]
fn test_versioned_transaction_roundtrip_and_dedupe() {
    // Construct a message with repeated pubkeys to exercise dedupe
    let k = Pubkey::new_unique();
    let header = MessageHeader {
        num_required_signatures: 1,
        num_readonly_signed_accounts: 0,
        num_readonly_unsigned_accounts: 2,
    };
    let account_keys = vec![k, k, k];
    let recent_blockhash = Hash::new_unique();
    let instructions = vec![CompiledInstruction {
        program_id_index: 2,
        accounts: vec![0, 1],
        data: vec![0xAA],
    }];
    let message = VersionedMessage::Legacy(Message {
        header,
        account_keys,
        recent_blockhash,
        instructions,
    });
    let tx = VersionedTransaction {
        signatures: vec![Signature::default()],
        message,
    };

    // Encode without dedupe
    let mut buf_plain = Vec::new();
    tx.encode_ext(&mut buf_plain, None).unwrap();

    // Encode with dedupe
    let mut enc = DedupeEncoder::new();
    let mut buf_dedupe = Vec::new();
    tx.encode_ext(&mut buf_dedupe, Some(&mut enc)).unwrap();
    assert!(buf_dedupe.len() < buf_plain.len());

    // Round-trip with decoder
    let mut dec = DedupeDecoder::new();
    let tx_dec =
        VersionedTransaction::decode_ext(&mut std::io::Cursor::new(&buf_dedupe), Some(&mut dec))
            .unwrap();
    assert_eq!(tx, tx_dec);
}

// removed obsolete placeholder impl for SanitizedTransaction

#[test]
fn test_encode_decode_sanitized_message() {
    let header = MessageHeader {
        num_required_signatures: 2,
        num_readonly_signed_accounts: 1,
        num_readonly_unsigned_accounts: 1,
    };
    let account_keys = vec![
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        Pubkey::new_unique(),
    ];
    let recent_blockhash = Hash::new_unique();
    let instructions = vec![
        CompiledInstruction {
            program_id_index: 0,
            accounts: vec![1, 2],
            data: vec![1, 2, 3],
        },
        CompiledInstruction {
            program_id_index: 1,
            accounts: vec![0, 3],
            data: vec![4, 5, 6],
        },
    ];
    let message = Message {
        header,
        account_keys,
        recent_blockhash,
        instructions,
    };
    let legacy_message = LegacyMessage {
        message: std::borrow::Cow::Owned(message),
        is_writable_account_cache: vec![true, false, true, false],
    };
    let original = SanitizedMessage::Legacy(legacy_message);

    let mut buffer = Vec::new();
    let bytes_written = original.encode(&mut buffer).unwrap();
    assert!(bytes_written > 0);

    let mut cursor = Cursor::new(&buffer);
    let decoded: SanitizedMessage = SanitizedMessage::decode(&mut cursor).unwrap();

    match (&original, &decoded) {
        (SanitizedMessage::Legacy(orig), SanitizedMessage::Legacy(decoded)) => {
            assert_eq!(orig.message, decoded.message);
            assert_eq!(
                orig.is_writable_account_cache,
                decoded.is_writable_account_cache
            );
        }
        _ => panic!("Decoded variant does not match original"),
    }
}

#[test]
fn test_encode_decode_sanitized_transaction_legacy() {
    // Build a simple legacy message
    let header = MessageHeader {
        num_required_signatures: 2,
        num_readonly_signed_accounts: 0,
        num_readonly_unsigned_accounts: 1,
    };
    let account_keys = vec![
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        Pubkey::new_unique(),
    ];
    let recent_blockhash = Hash::new_unique();
    let instructions = vec![CompiledInstruction {
        program_id_index: 0,
        accounts: vec![1, 2],
        data: vec![9, 8, 7],
    }];
    let message = Message {
        header,
        account_keys,
        recent_blockhash,
        instructions,
    };

    let is_writable_account_cache = vec![true, false, false];
    let legacy_message = LegacyMessage {
        message: std::borrow::Cow::Owned(message),
        is_writable_account_cache,
    };

    let sanitized = SanitizedMessage::Legacy(legacy_message);
    let signatures = vec![Signature::default(), Signature::default()];
    let tx =
        SanitizedTransaction::try_new_from_fields(sanitized, Hash::new_unique(), false, signatures)
            .unwrap();

    // Round-trip encode/decode
    let mut buf = Vec::new();
    tx.encode(&mut buf).unwrap();
    let decoded = SanitizedTransaction::decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(tx, decoded);
}

#[test]
fn test_encode_decode_sanitized_transaction_v0() {
    // Build a simple v0 message with loaded addresses
    let header = MessageHeader {
        num_required_signatures: 1,
        num_readonly_signed_accounts: 0,
        num_readonly_unsigned_accounts: 1,
    };
    let account_keys = vec![Pubkey::new_unique(), Pubkey::new_unique()];
    let recent_blockhash = Hash::new_unique();
    let instructions = vec![CompiledInstruction {
        program_id_index: 1,
        accounts: vec![0],
        data: vec![1, 2, 3, 4],
    }];
    let address_table_lookups = Vec::<MessageAddressTableLookup>::new();
    let msg = v0::Message {
        header,
        account_keys,
        recent_blockhash,
        instructions,
        address_table_lookups,
    };
    let loaded_addresses = v0::LoadedAddresses {
        writable: vec![Pubkey::new_unique()],
        readonly: vec![Pubkey::new_unique()],
    };
    let sanitized_v0 =
        v0::LoadedMessage::new(msg, loaded_addresses, &std::collections::HashSet::default());
    let sanitized = SanitizedMessage::V0(sanitized_v0);

    let signatures = vec![Signature::default()];
    let tx =
        SanitizedTransaction::try_new_from_fields(sanitized, Hash::new_unique(), false, signatures)
            .unwrap();

    // Round-trip encode/decode
    let mut buf = Vec::new();
    tx.encode(&mut buf).unwrap();
    let decoded = SanitizedTransaction::decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(tx, decoded);
}

#[test]
fn test_sanitized_transaction_legacy_with_dedup() {
    // Create repeated pubkeys to exercise dedupe
    let k1 = Pubkey::new_unique();
    let k2 = Pubkey::new_unique();
    let k3 = k1; // repeat

    let header = MessageHeader {
        num_required_signatures: 2,
        num_readonly_signed_accounts: 0,
        num_readonly_unsigned_accounts: 1,
    };
    let account_keys = vec![k1, k2, k3, k2, k1];
    let recent_blockhash = Hash::new_unique();
    let instructions = vec![CompiledInstruction {
        program_id_index: 0,
        accounts: vec![1, 2, 3],
        data: vec![42, 1, 2, 3],
    }];
    let message = Message {
        header,
        account_keys,
        recent_blockhash,
        instructions,
    };
    let is_writable_account_cache = vec![true, false, false, false, true];
    let legacy_message = LegacyMessage {
        message: std::borrow::Cow::Owned(message),
        is_writable_account_cache,
    };
    let sanitized = SanitizedMessage::Legacy(legacy_message);
    let tx = SanitizedTransaction::try_new_from_fields(
        sanitized,
        Hash::new_unique(),
        false,
        vec![Signature::default(), Signature::default()],
    )
    .unwrap();

    let mut enc = DedupeEncoder::new();
    let mut buf1 = Vec::new();
    tx.encode_ext(&mut buf1, Some(&mut enc)).unwrap();

    // Encoding the same tx with the same encoder should be smaller since pubkeys are deduped
    let mut buf2 = Vec::new();
    tx.encode_ext(&mut buf2, Some(&mut enc)).unwrap();
    assert!(buf2.len() < buf1.len());

    // Round-trip decode both using a shared decoder to respect IDs
    let mut dec = DedupeDecoder::new();
    let tx1 = SanitizedTransaction::decode_ext(&mut Cursor::new(&buf1), Some(&mut dec)).unwrap();
    let tx2 = SanitizedTransaction::decode_ext(&mut Cursor::new(&buf2), Some(&mut dec)).unwrap();
    assert_eq!(tx, tx1);
    assert_eq!(tx, tx2);
}

#[test]
fn test_sanitized_transaction_v0_with_dedup() {
    // Repeated pubkeys across account_keys and loaded addresses
    let k1 = Pubkey::new_unique();
    let k2 = Pubkey::new_unique();

    let header = MessageHeader {
        num_required_signatures: 1,
        num_readonly_signed_accounts: 0,
        num_readonly_unsigned_accounts: 1,
    };
    let account_keys = vec![k1, k2, k1, k2];
    let recent_blockhash = Hash::new_unique();
    let instructions = vec![CompiledInstruction {
        program_id_index: 1,
        accounts: vec![0, 2],
        data: vec![7, 7, 7],
    }];
    let address_table_lookups = vec![];
    let msg = v0::Message {
        header,
        account_keys,
        recent_blockhash,
        instructions,
        address_table_lookups,
    };
    let loaded_addresses = v0::LoadedAddresses {
        writable: vec![k1, k2, k1],
        readonly: vec![k2],
    };
    let loaded =
        v0::LoadedMessage::new(msg, loaded_addresses, &std::collections::HashSet::default());
    let sanitized = SanitizedMessage::V0(loaded);
    let tx = SanitizedTransaction::try_new_from_fields(
        sanitized,
        Hash::new_unique(),
        false,
        vec![Signature::default()],
    )
    .unwrap();

    let mut enc = DedupeEncoder::new();
    let mut buf1 = Vec::new();
    tx.encode_ext(&mut buf1, Some(&mut enc)).unwrap();
    let mut buf2 = Vec::new();
    tx.encode_ext(&mut buf2, Some(&mut enc)).unwrap();
    assert!(buf2.len() < buf1.len());

    let mut dec = DedupeDecoder::new();
    let tx1 = SanitizedTransaction::decode_ext(&mut Cursor::new(&buf1), Some(&mut dec)).unwrap();
    let tx2 = SanitizedTransaction::decode_ext(&mut Cursor::new(&buf2), Some(&mut dec)).unwrap();
    assert_eq!(tx, tx1);
    assert_eq!(tx, tx2);
}

#[test]
fn test_encode_decode_legacy_message() {
    for _ in 0..1000 {
        let header = MessageHeader {
            num_required_signatures: rand::random::<u8>(),
            num_readonly_signed_accounts: rand::random::<u8>(),
            num_readonly_unsigned_accounts: rand::random::<u8>(),
        };
        let account_keys: Vec<Pubkey> = (0..rand::random::<u8>() % 10)
            .map(|_| Pubkey::new_unique())
            .collect();
        let recent_blockhash = Hash::new_unique();
        let instructions: Vec<CompiledInstruction> = (0..rand::random::<u8>() % 5)
            .map(|_| CompiledInstruction {
                program_id_index: rand::random::<u8>(),
                accounts: (0..rand::random::<u8>() % 10)
                    .map(|_| rand::random::<u8>())
                    .collect(),
                data: (0..rand::random::<u8>() % 20)
                    .map(|_| rand::random::<u8>())
                    .collect(),
            })
            .collect();

        let message = Message {
            header,
            account_keys,
            recent_blockhash,
            instructions,
        };

        let is_writable_account_cache: Vec<bool> = (0..message.account_keys.len())
            .map(|_| rand::random::<bool>())
            .collect();

        let legacy_message = LegacyMessage {
            message: std::borrow::Cow::Owned(message),
            is_writable_account_cache,
        };

        let mut buf = [0u8; 1024];
        let mut cursor = Cursor::new(&mut buf);
        legacy_message.encode_ext(&mut cursor, None).unwrap();
        let decoded_legacy_message =
            LegacyMessage::decode_ext(&mut Cursor::new(&buf), None).unwrap();
        assert_eq!(legacy_message.message, decoded_legacy_message.message);
        assert_eq!(
            legacy_message.is_writable_account_cache,
            decoded_legacy_message.is_writable_account_cache
        );
    }
}

#[test]
fn test_encode_decode_v0_message() {
    for _ in 0..1000 {
        let header = MessageHeader {
            num_required_signatures: rand::random::<u8>(),
            num_readonly_signed_accounts: rand::random::<u8>(),
            num_readonly_unsigned_accounts: rand::random::<u8>(),
        };
        let account_keys: Vec<Pubkey> = (0..rand::random::<u8>() % 10)
            .map(|_| Pubkey::new_unique())
            .collect();
        let recent_blockhash = Hash::new_unique();
        let instructions: Vec<CompiledInstruction> = (0..rand::random::<u8>() % 5)
            .map(|_| CompiledInstruction {
                program_id_index: rand::random::<u8>(),
                accounts: (0..rand::random::<u8>() % 10)
                    .map(|_| rand::random::<u8>())
                    .collect(),
                data: (0..rand::random::<u8>() % 20)
                    .map(|_| rand::random::<u8>())
                    .collect(),
            })
            .collect();
        let address_table_lookups: Vec<MessageAddressTableLookup> = (0..rand::random::<u8>() % 3)
            .map(|_| MessageAddressTableLookup {
                account_key: Pubkey::new_unique(),
                writable_indexes: (0..rand::random::<u8>() % 5)
                    .map(|_| rand::random::<u8>())
                    .collect(),
                readonly_indexes: (0..rand::random::<u8>() % 5)
                    .map(|_| rand::random::<u8>())
                    .collect(),
            })
            .collect();

        let message = v0::Message {
            header,
            account_keys,
            recent_blockhash,
            instructions,
            address_table_lookups,
        };

        let mut buf = [0u8; 1024];
        let mut cursor = Cursor::new(&mut buf);
        message.encode_ext(&mut cursor, None).unwrap();
        let decoded_message = v0::Message::decode_ext(&mut Cursor::new(&buf), None).unwrap();
        assert_eq!(message, decoded_message);
    }
}

#[test]
fn test_encode_decode_message() {
    for _ in 0..1000 {
        let header = MessageHeader {
            num_required_signatures: rand::random::<u8>(),
            num_readonly_signed_accounts: rand::random::<u8>(),
            num_readonly_unsigned_accounts: rand::random::<u8>(),
        };
        let account_keys: Vec<Pubkey> = (0..rand::random::<u8>() % 10)
            .map(|_| Pubkey::new_unique())
            .collect();
        let recent_blockhash = Hash::new_unique();
        let instructions: Vec<CompiledInstruction> = (0..rand::random::<u8>() % 5)
            .map(|_| CompiledInstruction {
                program_id_index: rand::random::<u8>(),
                accounts: (0..rand::random::<u8>() % 10)
                    .map(|_| rand::random::<u8>())
                    .collect(),
                data: (0..rand::random::<u8>() % 20)
                    .map(|_| rand::random::<u8>())
                    .collect(),
            })
            .collect();

        let message = Message {
            header,
            account_keys,
            recent_blockhash,
            instructions,
        };

        let mut buf = [0u8; 512];
        let mut cursor = Cursor::new(&mut buf);
        message.encode_ext(&mut cursor, None).unwrap();
        let decoded_message = Message::decode_ext(&mut Cursor::new(&buf), None).unwrap();
        assert_eq!(message, decoded_message);
    }
}

#[test]
fn test_encode_decode_compiled_instruction() {
    for _ in 0..1000 {
        let instruction = CompiledInstruction {
            program_id_index: rand::random::<u8>(),
            accounts: (0..rand::random::<u8>() % 10)
                .map(|_| rand::random::<u8>())
                .collect(),
            data: (0..rand::random::<u8>() % 20)
                .map(|_| rand::random::<u8>())
                .collect(),
        };
        let mut buf = [0u8; 100];
        let mut cursor = Cursor::new(&mut buf);
        instruction.encode_ext(&mut cursor, None).unwrap();
        let decoded_instruction =
            CompiledInstruction::decode_ext(&mut Cursor::new(&buf), None).unwrap();
        assert_eq!(instruction, decoded_instruction);
    }
}

#[test]
fn test_encode_decode_pubkey() {
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
    use {DedupeDecoder, DedupeEncoder};

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
    use {DedupeDecoder, DedupeEncoder};

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
