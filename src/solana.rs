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

// ========== solana-sdk (v2) message primitives ==========

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
        let b = combined.to_le_bytes();
        Ok(Self {
            num_required_signatures: b[0],
            num_readonly_signed_accounts: b[1],
            num_readonly_unsigned_accounts: b[2],
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
        let mut n = 0;
        n += self
            .program_id_index
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .accounts
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self.data.encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}

impl Decode for CompiledInstruction {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let program_id_index: u8 = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let accounts: Vec<u8> = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let data: Vec<u8> = Decode::decode_ext(reader, dedupe_decoder)?;
        Ok(Self {
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
        let mut n = 0;
        n += self
            .header
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .account_keys
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .recent_blockhash
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self.instructions.encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}

impl Decode for Message {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let header: MessageHeader = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let account_keys: Vec<Pubkey> = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let recent_blockhash: Hash = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let instructions: Vec<CompiledInstruction> = Decode::decode_ext(reader, dedupe_decoder)?;
        Ok(Self {
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
        let mut n = 0;
        n += self
            .account_key
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .writable_indexes
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self.readonly_indexes.encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}

impl Decode for MessageAddressTableLookup {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let account_key: Pubkey = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let writable_indexes: Vec<u8> = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let readonly_indexes: Vec<u8> = Decode::decode_ext(reader, dedupe_decoder)?;
        Ok(Self {
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
        let mut n = 0;
        n += self
            .header
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .account_keys
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .recent_blockhash
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .instructions
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .address_table_lookups
            .encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}

impl Decode for v0::Message {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let header: MessageHeader = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let account_keys: Vec<Pubkey> = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let recent_blockhash: Hash = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let instructions: Vec<CompiledInstruction> =
            Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let address_table_lookups: Vec<MessageAddressTableLookup> =
            Decode::decode_ext(reader, dedupe_decoder)?;
        Ok(Self {
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
        let mut n = 0;
        n += self
            .message
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .is_writable_account_cache
            .encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}

impl Decode for LegacyMessage<'_> {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let message: Message = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let is_writable_account_cache: Vec<bool> = Decode::decode_ext(reader, dedupe_decoder)?;
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
        match self {
            SanitizedMessage::Legacy(inner) => {
                let mut n = <usize as Encode>::encode_discriminant(0, writer)?;
                n += inner.encode_ext(writer, dedupe_encoder)?;
                Ok(n)
            }
            SanitizedMessage::V0(inner) => {
                let mut n = <usize as Encode>::encode_discriminant(1, writer)?;
                n += inner.encode_ext(writer, dedupe_encoder)?;
                Ok(n)
            }
        }
    }
}

// Implementations for Agave (v3) Geyser interface and its dependencies (inline)
use agave_geyser_plugin_interface::geyser_plugin_interface as ifc;
use solana_account_decoder_client_types as acct_dec_client;
use solana_clock as clock;
use solana_hash as hash3;
use solana_message as msg3;
use solana_pubkey as pubkey3;
use solana_reward_info as reward_info;
use solana_signature as sig3;
use solana_transaction as tx3;
use solana_transaction_context as txctx3;
use solana_transaction_error as txerr3;
use solana_transaction_status as txstatus3;

// Small serde helpers when beneficial
#[inline(always)]
fn encode_serde_blob<T: serde::Serialize>(value: &T, writer: &mut impl Write) -> Result<usize> {
    let data = bincode::serde::encode_to_vec(value, bincode::config::standard())
        .map_err(|_| Error::InvalidData)?;
    let mut written = 0;
    written += <usize as Encode>::encode_len(data.len(), writer)?;
    written += writer.write(&data)?;
    Ok(written)
}

#[inline(always)]
fn decode_serde_blob<T: serde::de::DeserializeOwned>(reader: &mut impl Read) -> Result<T> {
    let len = <usize as Decode>::decode_len(reader)?;
    let mut buf = vec![0u8; len];
    if reader.read(&mut buf)? != len {
        return Err(Error::ReaderOutOfData);
    }
    let (v, _): (T, usize) = bincode::serde::decode_from_slice(&buf, bincode::config::standard())
        .map_err(|_| Error::InvalidData)?;
    Ok(v)
}

// Pubkey/Hash/Signature for v3 crates
impl Pack for pubkey3::Pubkey {
    #[inline(always)]
    fn pack(&self, writer: &mut impl Write) -> Result<usize> {
        self.to_bytes().pack(writer)
    }
    #[inline(always)]
    fn unpack(reader: &mut impl Read) -> Result<Self> {
        let mut buf = [0u8; 32];
        if reader.read(&mut buf)? != 32 {
            return Err(Error::ReaderOutOfData);
        }
        Ok(Self::new_from_array(buf))
    }
}
impl DedupeEncodeable for pubkey3::Pubkey {}
impl DedupeDecodeable for pubkey3::Pubkey {}

impl Encode for hash3::Hash {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        self.as_bytes().encode_ext(writer, dedupe_encoder)
    }
}
impl Decode for hash3::Hash {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let bytes = <[u8; hash3::HASH_BYTES]>::decode_ext(reader, dedupe_decoder)?;
        Ok(Self::new_from_array(bytes))
    }
}
impl Encode for sig3::Signature {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        self.as_array().encode_ext(writer, dedupe_encoder)
    }
}
impl Decode for sig3::Signature {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        _dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let sig: [u8; sig3::SIGNATURE_BYTES] = decode(reader)?;
        Ok(Self::from(sig))
    }
}

// Message components (v3)
impl Encode for msg3::MessageHeader {
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
impl Decode for msg3::MessageHeader {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        _dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let combined: u32 = decode(reader)?;
        let b = combined.to_le_bytes();
        Ok(Self {
            num_required_signatures: b[0],
            num_readonly_signed_accounts: b[1],
            num_readonly_unsigned_accounts: b[2],
        })
    }
}

impl Encode for msg3::compiled_instruction::CompiledInstruction {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self
            .program_id_index
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .accounts
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self.data.encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}
impl Decode for msg3::compiled_instruction::CompiledInstruction {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let program_id_index: u8 = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let accounts: Vec<u8> = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let data: Vec<u8> = Decode::decode_ext(reader, dedupe_decoder)?;
        Ok(Self {
            program_id_index,
            accounts,
            data,
        })
    }
}

impl Encode for msg3::legacy::Message {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self
            .header
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .account_keys
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .recent_blockhash
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self.instructions.encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}
impl Decode for msg3::legacy::Message {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let header = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let account_keys = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let recent_blockhash = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let instructions = Decode::decode_ext(reader, dedupe_decoder)?;
        Ok(Self {
            header,
            account_keys,
            recent_blockhash,
            instructions,
        })
    }
}
impl Encode for msg3::v0::MessageAddressTableLookup {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self
            .account_key
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .writable_indexes
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self.readonly_indexes.encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}
impl Decode for msg3::v0::MessageAddressTableLookup {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let account_key = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let writable_indexes = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let readonly_indexes = Decode::decode_ext(reader, dedupe_decoder)?;
        Ok(Self {
            account_key,
            writable_indexes,
            readonly_indexes,
        })
    }
}
impl Encode for msg3::v0::Message {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self
            .header
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .account_keys
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .recent_blockhash
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .instructions
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .address_table_lookups
            .encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}
impl Decode for msg3::v0::Message {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let header = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let account_keys = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let recent_blockhash = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let instructions = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let address_table_lookups = Decode::decode_ext(reader, dedupe_decoder)?;
        Ok(Self {
            header,
            account_keys,
            recent_blockhash,
            instructions,
            address_table_lookups,
        })
    }
}

// Encode/Decode for sanitized LegacyMessage wrapper (v3)
impl Encode for msg3::LegacyMessage<'_> {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self
            .message
            .as_ref()
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .is_writable_account_cache
            .encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}
impl Decode for msg3::LegacyMessage<'_> {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let message = msg3::legacy::Message::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let is_writable_account_cache = Vec::<bool>::decode_ext(reader, dedupe_decoder)?;
        Ok(Self {
            message: std::borrow::Cow::Owned(message),
            is_writable_account_cache,
        })
    }
}

impl Encode for msg3::SanitizedMessage {
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        match self {
            msg3::SanitizedMessage::Legacy(m) => {
                let mut n = 0;
                n += <usize as Encode>::encode_discriminant(0, writer)?;
                n += m.encode_ext(writer, dedupe_encoder)?;
                Ok(n)
            }
            msg3::SanitizedMessage::V0(m) => {
                let mut n = 0;
                n += <usize as Encode>::encode_discriminant(1, writer)?;
                n += m.encode_ext(writer, dedupe_encoder)?;
                Ok(n)
            }
        }
    }
}
impl Decode for msg3::SanitizedMessage {
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        match <usize as Decode>::decode_discriminant(reader)? {
            0 => Ok(Self::Legacy(Decode::decode_ext(
                reader,
                dedupe_decoder.as_deref_mut(),
            )?)),
            1 => Ok(Self::V0(Decode::decode_ext(reader, dedupe_decoder)?)),
            _ => Err(Error::InvalidData),
        }
    }
}

impl Encode for msg3::v0::LoadedAddresses {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self
            .writable
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self.readonly.encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}
impl Decode for msg3::v0::LoadedAddresses {
    #[inline(always)]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let writable = Vec::<pubkey3::Pubkey>::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let readonly = Vec::<pubkey3::Pubkey>::decode_ext(reader, dedupe_decoder)?;
        Ok(Self { writable, readonly })
    }
}
impl<'a> Encode for msg3::v0::LoadedMessage<'a> {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self
            .message
            .as_ref()
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .loaded_addresses
            .as_ref()
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .is_writable_account_cache
            .encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}
impl<'a> Decode for msg3::v0::LoadedMessage<'a> {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let msg = msg3::v0::Message::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let addrs = msg3::v0::LoadedAddresses::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let cache = Vec::<bool>::decode_ext(reader, dedupe_decoder)?;
        Ok(Self {
            message: std::borrow::Cow::Owned(msg),
            loaded_addresses: std::borrow::Cow::Owned(addrs),
            is_writable_account_cache: cache,
        })
    }
}

// VersionedMessage and transactions (v3)
impl Encode for msg3::VersionedMessage {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        match self {
            msg3::VersionedMessage::Legacy(m) => {
                n += <usize as Encode>::encode_discriminant(0, writer)?;
                n += m.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
            }
            msg3::VersionedMessage::V0(m) => {
                n += <usize as Encode>::encode_discriminant(1, writer)?;
                n += m.encode_ext(writer, dedupe_encoder)?;
            }
        }
        Ok(n)
    }
}
impl Decode for msg3::VersionedMessage {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        match <usize as Decode>::decode_discriminant(reader)? {
            0 => Ok(Self::Legacy(Decode::decode_ext(
                reader,
                dedupe_decoder.as_deref_mut(),
            )?)),
            1 => Ok(Self::V0(Decode::decode_ext(reader, dedupe_decoder)?)),
            _ => Err(Error::InvalidData),
        }
    }
}
impl Encode for tx3::versioned::VersionedTransaction {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self
            .signatures
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self.message.encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}
impl Decode for tx3::versioned::VersionedTransaction {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let signatures = Vec::<sig3::Signature>::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let message = msg3::VersionedMessage::decode_ext(reader, dedupe_decoder)?;
        Ok(Self {
            signatures,
            message,
        })
    }
}
impl Encode for tx3::sanitized::SanitizedTransaction {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self
            .message()
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .message_hash()
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .is_simple_vote_transaction()
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        let sigs: Vec<sig3::Signature> = self.signatures().to_vec();
        n += sigs.encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}
impl Decode for tx3::sanitized::SanitizedTransaction {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let message = msg3::SanitizedMessage::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let message_hash = hash3::Hash::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let is_simple_vote_tx = bool::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let signatures = Vec::<sig3::Signature>::decode_ext(reader, dedupe_decoder)?;
        tx3::sanitized::SanitizedTransaction::try_new_from_fields(
            message,
            message_hash,
            is_simple_vote_tx,
            signatures,
        )
        .map_err(|_| Error::InvalidData)
    }
}

// TransactionStatusMeta and friends
impl Encode for txstatus3::InnerInstruction {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self
            .instruction
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self.stack_height.encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}
impl Decode for txstatus3::InnerInstruction {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let instruction = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let stack_height = Decode::decode_ext(reader, dedupe_decoder)?;
        Ok(Self {
            instruction,
            stack_height,
        })
    }
}
impl Encode for txstatus3::InnerInstructions {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self
            .index
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self.instructions.encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}
impl Decode for txstatus3::InnerInstructions {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        let index = Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?;
        let instructions = Decode::decode_ext(reader, dedupe_decoder)?;
        Ok(Self {
            index,
            instructions,
        })
    }
}
impl Encode for acct_dec_client::token::UiTokenAmount {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self
            .ui_amount
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .decimals
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .amount
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self.ui_amount_string.encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}
impl Decode for acct_dec_client::token::UiTokenAmount {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        Ok(Self {
            ui_amount: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            decimals: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            amount: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            ui_amount_string: Decode::decode_ext(reader, dedupe_decoder)?,
        })
    }
}

impl Encode for txstatus3::TransactionTokenBalance {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self
            .account_index
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .mint
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .ui_token_amount
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .owner
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self.program_id.encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}
impl Decode for txstatus3::TransactionTokenBalance {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        Ok(Self {
            account_index: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            mint: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            ui_token_amount: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            owner: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            program_id: Decode::decode_ext(reader, dedupe_decoder)?,
        })
    }
}

impl Encode for reward_info::RewardType {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        _dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let disc = match self {
            reward_info::RewardType::Fee => 0usize,
            reward_info::RewardType::Rent => 1,
            reward_info::RewardType::Staking => 2,
            reward_info::RewardType::Voting => 3,
        };
        <usize as Encode>::encode_discriminant(disc, writer)
    }
}
impl Decode for reward_info::RewardType {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        _dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        Ok(match <usize as Decode>::decode_discriminant(reader)? {
            0 => reward_info::RewardType::Fee,
            1 => reward_info::RewardType::Rent,
            2 => reward_info::RewardType::Staking,
            3 => reward_info::RewardType::Voting,
            _ => return Err(Error::InvalidData),
        })
    }
}
impl Encode for txstatus3::Reward {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self
            .pubkey
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .lamports
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .post_balance
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .reward_type
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self.commission.encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}
impl Decode for txstatus3::Reward {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        Ok(Self {
            pubkey: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            lamports: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            post_balance: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            reward_type: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            commission: Decode::decode_ext(reader, dedupe_decoder)?,
        })
    }
}
impl Encode for txstatus3::RewardsAndNumPartitions {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self
            .rewards
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self.num_partitions.encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}
impl Decode for txstatus3::RewardsAndNumPartitions {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        Ok(Self {
            rewards: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            num_partitions: Decode::decode_ext(reader, dedupe_decoder)?,
        })
    }
}
impl Encode for txctx3::TransactionReturnData {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self
            .program_id
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self.data.encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}
impl Decode for txctx3::TransactionReturnData {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        Ok(Self {
            program_id: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            data: Decode::decode_ext(reader, dedupe_decoder)?,
        })
    }
}
// TransactionError — use serde blob to avoid duplicating variants here
impl Encode for txerr3::TransactionError {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        _dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        encode_serde_blob(self, writer)
    }
}
impl Decode for txerr3::TransactionError {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        _dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        decode_serde_blob(reader)
    }
}
impl Encode for txstatus3::TransactionStatusMeta {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self
            .status
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self.fee.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .pre_balances
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .post_balances
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .inner_instructions
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .log_messages
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .pre_token_balances
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .post_token_balances
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .rewards
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .loaded_addresses
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .return_data
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self
            .compute_units_consumed
            .encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        n += self.cost_units.encode_ext(writer, dedupe_encoder)?;
        Ok(n)
    }
}
impl Decode for txstatus3::TransactionStatusMeta {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        Ok(Self {
            status: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            fee: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            pre_balances: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            post_balances: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            inner_instructions: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            log_messages: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            pre_token_balances: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            post_token_balances: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            rewards: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            loaded_addresses: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            return_data: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            compute_units_consumed: Decode::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
            cost_units: Decode::decode_ext(reader, dedupe_decoder)?,
        })
    }
}

// Geyser interface types
impl Encode for ifc::ReplicaAccountInfo<'_> {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        _dedupe: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self.pubkey.encode_ext(writer, None)?; // &[u8] Encode for slices is not defined; use Vec
        n += self.lamports.encode_ext(writer, None)?;
        n += self.owner.encode_ext(writer, None)?;
        n += self.executable.encode_ext(writer, None)?;
        n += self.rent_epoch.encode_ext(writer, None)?;
        n += self.data.encode_ext(writer, None)?;
        n += self.write_version.encode_ext(writer, None)?;
        Ok(n)
    }
}
impl Decode for ifc::ReplicaAccountInfo<'static> {
    #[inline]
    fn decode_ext(reader: &mut impl Read, _dedupe: Option<&mut DedupeDecoder>) -> Result<Self> {
        let pubkey: Vec<u8> = Decode::decode_ext(reader, None)?;
        let lamports = Decode::decode_ext(reader, None)?;
        let owner: Vec<u8> = Decode::decode_ext(reader, None)?;
        let executable = Decode::decode_ext(reader, None)?;
        let rent_epoch = Decode::decode_ext(reader, None)?;
        let data: Vec<u8> = Decode::decode_ext(reader, None)?;
        let write_version = Decode::decode_ext(reader, None)?;
        Ok(Self {
            pubkey: Box::leak(pubkey.into_boxed_slice()),
            lamports,
            owner: Box::leak(owner.into_boxed_slice()),
            executable,
            rent_epoch,
            data: Box::leak(data.into_boxed_slice()),
            write_version,
        })
    }
}
impl Encode for ifc::ReplicaAccountInfoV2<'_> {
    #[inline]
    fn encode_ext(&self, w: &mut impl Write, dedupe: Option<&mut DedupeEncoder>) -> Result<usize> {
        let mut n = ifc::ReplicaAccountInfo {
            pubkey: self.pubkey,
            lamports: self.lamports,
            owner: self.owner,
            executable: self.executable,
            rent_epoch: self.rent_epoch,
            data: self.data,
            write_version: self.write_version,
        }
        .encode_ext(w, None)?;
        let sig_opt: Option<sig3::Signature> = self.txn_signature.copied();
        n += sig_opt.encode_ext(w, dedupe)?;
        Ok(n)
    }
}
impl Decode for ifc::ReplicaAccountInfoV2<'static> {
    #[inline]
    fn decode_ext(reader: &mut impl Read, dedupe: Option<&mut DedupeDecoder>) -> Result<Self> {
        let base: ifc::ReplicaAccountInfo<'static> = Decode::decode_ext(reader, None)?;
        let txn_signature: Option<sig3::Signature> = Decode::decode_ext(reader, dedupe)?;
        let sig_ref = txn_signature.map(|s| Box::leak(Box::new(s)) as &sig3::Signature);
        Ok(Self {
            pubkey: base.pubkey,
            lamports: base.lamports,
            owner: base.owner,
            executable: base.executable,
            rent_epoch: base.rent_epoch,
            data: base.data,
            write_version: base.write_version,
            txn_signature: sig_ref,
        })
    }
}
impl Encode for ifc::ReplicaAccountInfoV3<'_> {
    #[inline]
    fn encode_ext(&self, w: &mut impl Write, dedupe: Option<&mut DedupeEncoder>) -> Result<usize> {
        let mut n = ifc::ReplicaAccountInfo {
            pubkey: self.pubkey,
            lamports: self.lamports,
            owner: self.owner,
            executable: self.executable,
            rent_epoch: self.rent_epoch,
            data: self.data,
            write_version: self.write_version,
        }
        .encode_ext(w, None)?;
        let tx_opt: Option<tx3::sanitized::SanitizedTransaction> = self.txn.cloned();
        n += tx_opt.encode_ext(w, dedupe)?;
        Ok(n)
    }
}
impl Decode for ifc::ReplicaAccountInfoV3<'static> {
    #[inline]
    fn decode_ext(reader: &mut impl Read, dedupe: Option<&mut DedupeDecoder>) -> Result<Self> {
        let base: ifc::ReplicaAccountInfo<'static> = Decode::decode_ext(reader, None)?;
        let txn: Option<tx3::sanitized::SanitizedTransaction> = Decode::decode_ext(reader, dedupe)?;
        let txn_ref = txn.map(|t| Box::leak(Box::new(t)) as &tx3::sanitized::SanitizedTransaction);
        Ok(Self {
            pubkey: base.pubkey,
            lamports: base.lamports,
            owner: base.owner,
            executable: base.executable,
            rent_epoch: base.rent_epoch,
            data: base.data,
            write_version: base.write_version,
            txn: txn_ref,
        })
    }
}
impl Encode for ifc::ReplicaAccountInfoVersions<'_> {
    #[inline]
    fn encode_ext(
        &self,
        w: &mut impl Write,
        mut dedupe: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        match self {
            ifc::ReplicaAccountInfoVersions::V0_0_1(v) => {
                let mut n = <usize as Encode>::encode_discriminant(0, w)?;
                n += (*v).encode_ext(w, dedupe.as_deref_mut())?;
                Ok(n)
            }
            ifc::ReplicaAccountInfoVersions::V0_0_2(v) => {
                let mut n = <usize as Encode>::encode_discriminant(1, w)?;
                n += (*v).encode_ext(w, dedupe.as_deref_mut())?;
                Ok(n)
            }
            ifc::ReplicaAccountInfoVersions::V0_0_3(v) => {
                let mut n = <usize as Encode>::encode_discriminant(2, w)?;
                n += (*v).encode_ext(w, dedupe)?;
                Ok(n)
            }
        }
    }
}
impl Decode for ifc::ReplicaAccountInfoVersions<'static> {
    #[inline]
    fn decode_ext(reader: &mut impl Read, dedupe: Option<&mut DedupeDecoder>) -> Result<Self> {
        match <usize as Decode>::decode_discriminant(reader)? {
            0 => Ok(Self::V0_0_1(Box::leak(Box::new(Decode::decode_ext(
                reader, dedupe,
            )?)))),
            1 => Ok(Self::V0_0_2(Box::leak(Box::new(Decode::decode_ext(
                reader, dedupe,
            )?)))),
            2 => Ok(Self::V0_0_3(Box::leak(Box::new(Decode::decode_ext(
                reader, dedupe,
            )?)))),
            _ => Err(Error::InvalidData),
        }
    }
}

impl Encode for ifc::ReplicaTransactionInfo<'_> {
    #[inline]
    fn encode_ext(
        &self,
        w: &mut impl Write,
        mut dedupe: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += (*self.signature).encode_ext(w, dedupe.as_deref_mut())?;
        n += self.is_vote.encode_ext(w, dedupe.as_deref_mut())?;
        n += (*self.transaction).encode_ext(w, dedupe.as_deref_mut())?;
        n += (*self.transaction_status_meta).encode_ext(w, dedupe)?;
        Ok(n)
    }
}
impl Decode for ifc::ReplicaTransactionInfo<'static> {
    #[inline]
    fn decode_ext(reader: &mut impl Read, mut dedupe: Option<&mut DedupeDecoder>) -> Result<Self> {
        let signature: sig3::Signature = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let is_vote: bool = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let transaction: tx3::sanitized::SanitizedTransaction =
            Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let transaction_status_meta: txstatus3::TransactionStatusMeta =
            Decode::decode_ext(reader, dedupe)?;
        Ok(Self {
            signature: Box::leak(Box::new(signature)),
            is_vote,
            transaction: Box::leak(Box::new(transaction)),
            transaction_status_meta: Box::leak(Box::new(transaction_status_meta)),
        })
    }
}
impl Encode for ifc::ReplicaTransactionInfoV2<'_> {
    #[inline]
    fn encode_ext(
        &self,
        w: &mut impl Write,
        mut dedupe: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let base = ifc::ReplicaTransactionInfo {
            signature: self.signature,
            is_vote: self.is_vote,
            transaction: self.transaction,
            transaction_status_meta: self.transaction_status_meta,
        };
        let mut n = base.encode_ext(w, dedupe.as_deref_mut())?;
        n += self.index.encode_ext(w, dedupe)?;
        Ok(n)
    }
}
impl Decode for ifc::ReplicaTransactionInfoV2<'static> {
    #[inline]
    fn decode_ext(reader: &mut impl Read, mut dedupe: Option<&mut DedupeDecoder>) -> Result<Self> {
        let base: ifc::ReplicaTransactionInfo<'static> =
            Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let index = Decode::decode_ext(reader, dedupe)?;
        Ok(Self {
            signature: base.signature,
            is_vote: base.is_vote,
            transaction: base.transaction,
            transaction_status_meta: base.transaction_status_meta,
            index,
        })
    }
}
impl Encode for ifc::ReplicaTransactionInfoV3<'_> {
    #[inline]
    fn encode_ext(
        &self,
        w: &mut impl Write,
        mut dedupe: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += (*self.signature).encode_ext(w, dedupe.as_deref_mut())?;
        n += (*self.message_hash).encode_ext(w, dedupe.as_deref_mut())?;
        n += self.is_vote.encode_ext(w, dedupe.as_deref_mut())?;
        n += (*self.transaction).encode_ext(w, dedupe.as_deref_mut())?;
        n += (*self.transaction_status_meta).encode_ext(w, dedupe.as_deref_mut())?;
        n += self.index.encode_ext(w, dedupe)?;
        Ok(n)
    }
}
impl Decode for ifc::ReplicaTransactionInfoV3<'static> {
    #[inline]
    fn decode_ext(reader: &mut impl Read, mut dedupe: Option<&mut DedupeDecoder>) -> Result<Self> {
        let signature: sig3::Signature = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let message_hash: hash3::Hash = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let is_vote: bool = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let transaction: tx3::versioned::VersionedTransaction =
            Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let transaction_status_meta: txstatus3::TransactionStatusMeta =
            Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let index: usize = Decode::decode_ext(reader, dedupe)?;
        Ok(Self {
            signature: Box::leak(Box::new(signature)),
            message_hash: Box::leak(Box::new(message_hash)),
            is_vote,
            transaction: Box::leak(Box::new(transaction)),
            transaction_status_meta: Box::leak(Box::new(transaction_status_meta)),
            index,
        })
    }
}
impl Encode for ifc::ReplicaTransactionInfoVersions<'_> {
    #[inline]
    fn encode_ext(
        &self,
        w: &mut impl Write,
        mut dedupe: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        match self {
            ifc::ReplicaTransactionInfoVersions::V0_0_1(v) => {
                let mut n = <usize as Encode>::encode_discriminant(0, w)?;
                n += (*v).encode_ext(w, dedupe.as_deref_mut())?;
                Ok(n)
            }
            ifc::ReplicaTransactionInfoVersions::V0_0_2(v) => {
                let mut n = <usize as Encode>::encode_discriminant(1, w)?;
                n += (*v).encode_ext(w, dedupe.as_deref_mut())?;
                Ok(n)
            }
            ifc::ReplicaTransactionInfoVersions::V0_0_3(v) => {
                let mut n = <usize as Encode>::encode_discriminant(2, w)?;
                n += (*v).encode_ext(w, dedupe)?;
                Ok(n)
            }
        }
    }
}
impl Decode for ifc::ReplicaTransactionInfoVersions<'static> {
    #[inline]
    fn decode_ext(reader: &mut impl Read, dedupe: Option<&mut DedupeDecoder>) -> Result<Self> {
        match <usize as Decode>::decode_discriminant(reader)? {
            0 => Ok(Self::V0_0_1(Box::leak(Box::new(Decode::decode_ext(
                reader, dedupe,
            )?)))),
            1 => Ok(Self::V0_0_2(Box::leak(Box::new(Decode::decode_ext(
                reader, dedupe,
            )?)))),
            2 => Ok(Self::V0_0_3(Box::leak(Box::new(Decode::decode_ext(
                reader, dedupe,
            )?)))),
            _ => Err(Error::InvalidData),
        }
    }
}

impl Encode for ifc::ReplicaEntryInfo<'_> {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self.slot.encode_ext(writer, dedupe.as_deref_mut())?;
        n += self.index.encode_ext(writer, dedupe.as_deref_mut())?;
        n += self.num_hashes.encode_ext(writer, dedupe.as_deref_mut())?;
        n += self.hash.encode_ext(writer, dedupe.as_deref_mut())?;
        n += self.executed_transaction_count.encode_ext(writer, dedupe)?;
        Ok(n)
    }
}
impl Decode for ifc::ReplicaEntryInfo<'static> {
    #[inline]
    fn decode_ext(reader: &mut impl Read, mut dedupe: Option<&mut DedupeDecoder>) -> Result<Self> {
        Ok(Self {
            slot: Decode::decode_ext(reader, dedupe.as_deref_mut())?,
            index: Decode::decode_ext(reader, dedupe.as_deref_mut())?,
            num_hashes: Decode::decode_ext(reader, dedupe.as_deref_mut())?,
            hash: Box::leak(
                Vec::<u8>::decode_ext(reader, dedupe.as_deref_mut())?.into_boxed_slice(),
            ),
            executed_transaction_count: Decode::decode_ext(reader, dedupe)?,
        })
    }
}
impl Encode for ifc::ReplicaEntryInfoV2<'_> {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let base = ifc::ReplicaEntryInfo {
            slot: self.slot,
            index: self.index,
            num_hashes: self.num_hashes,
            hash: self.hash,
            executed_transaction_count: self.executed_transaction_count,
        };
        let mut n = base.encode_ext(writer, dedupe.as_deref_mut())?;
        n += self.starting_transaction_index.encode_ext(writer, dedupe)?;
        Ok(n)
    }
}
impl Decode for ifc::ReplicaEntryInfoV2<'static> {
    #[inline]
    fn decode_ext(reader: &mut impl Read, mut dedupe: Option<&mut DedupeDecoder>) -> Result<Self> {
        let base: ifc::ReplicaEntryInfo<'static> =
            Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let starting_transaction_index = Decode::decode_ext(reader, dedupe)?;
        Ok(Self {
            slot: base.slot,
            index: base.index,
            num_hashes: base.num_hashes,
            hash: base.hash,
            executed_transaction_count: base.executed_transaction_count,
            starting_transaction_index,
        })
    }
}
impl Encode for ifc::ReplicaEntryInfoVersions<'_> {
    #[inline]
    fn encode_ext(
        &self,
        w: &mut impl Write,
        mut dedupe: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        match self {
            ifc::ReplicaEntryInfoVersions::V0_0_1(v) => {
                let mut n = <usize as Encode>::encode_discriminant(0, w)?;
                n += (*v).encode_ext(w, dedupe.as_deref_mut())?;
                Ok(n)
            }
            ifc::ReplicaEntryInfoVersions::V0_0_2(v) => {
                let mut n = <usize as Encode>::encode_discriminant(1, w)?;
                n += (*v).encode_ext(w, dedupe)?;
                Ok(n)
            }
        }
    }
}
impl Decode for ifc::ReplicaEntryInfoVersions<'static> {
    #[inline]
    fn decode_ext(reader: &mut impl Read, dedupe: Option<&mut DedupeDecoder>) -> Result<Self> {
        match <usize as Decode>::decode_discriminant(reader)? {
            0 => Ok(Self::V0_0_1(Box::leak(Box::new(Decode::decode_ext(
                reader, dedupe,
            )?)))),
            1 => Ok(Self::V0_0_2(Box::leak(Box::new(Decode::decode_ext(
                reader, dedupe,
            )?)))),
            _ => Err(Error::InvalidData),
        }
    }
}

impl Encode for ifc::ReplicaBlockInfo<'_> {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self.slot.encode_ext(writer, dedupe.as_deref_mut())?;
        n += self.blockhash.encode_ext(writer, dedupe.as_deref_mut())?;
        n += self
            .rewards
            .to_vec()
            .encode_ext(writer, dedupe.as_deref_mut())?;
        n += self.block_time.encode_ext(writer, dedupe.as_deref_mut())?;
        n += self.block_height.encode_ext(writer, dedupe)?;
        Ok(n)
    }
}
impl Decode for ifc::ReplicaBlockInfo<'static> {
    #[inline]
    fn decode_ext(reader: &mut impl Read, mut dedupe: Option<&mut DedupeDecoder>) -> Result<Self> {
        let slot: clock::Slot = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let blockhash: String = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let rewards: Vec<txstatus3::Reward> = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let block_time: Option<i64> = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let block_height: Option<u64> = Decode::decode_ext(reader, dedupe)?;
        Ok(Self {
            slot,
            blockhash: Box::leak(blockhash.into_boxed_str()),
            rewards: Box::leak(rewards.into_boxed_slice()),
            block_time,
            block_height,
        })
    }
}
impl Encode for ifc::ReplicaBlockInfoV2<'_> {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self.parent_slot.encode_ext(writer, dedupe.as_deref_mut())?;
        n += self
            .parent_blockhash
            .encode_ext(writer, dedupe.as_deref_mut())?;
        n += self.slot.encode_ext(writer, dedupe.as_deref_mut())?;
        n += self.blockhash.encode_ext(writer, dedupe.as_deref_mut())?;
        n += self
            .rewards
            .to_vec()
            .encode_ext(writer, dedupe.as_deref_mut())?;
        n += self.block_time.encode_ext(writer, dedupe.as_deref_mut())?;
        n += self
            .block_height
            .encode_ext(writer, dedupe.as_deref_mut())?;
        n += self.executed_transaction_count.encode_ext(writer, dedupe)?;
        Ok(n)
    }
}
impl Decode for ifc::ReplicaBlockInfoV2<'static> {
    #[inline]
    fn decode_ext(reader: &mut impl Read, mut dedupe: Option<&mut DedupeDecoder>) -> Result<Self> {
        let parent_slot = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let parent_blockhash: String = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let slot = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let blockhash: String = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let rewards: Vec<txstatus3::Reward> = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let block_time = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let block_height = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let executed_transaction_count = Decode::decode_ext(reader, dedupe)?;
        Ok(Self {
            parent_slot,
            parent_blockhash: Box::leak(parent_blockhash.into_boxed_str()),
            slot,
            blockhash: Box::leak(blockhash.into_boxed_str()),
            rewards: Box::leak(rewards.into_boxed_slice()),
            block_time,
            block_height,
            executed_transaction_count,
        })
    }
}
impl Encode for ifc::ReplicaBlockInfoV3<'_> {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = ifc::ReplicaBlockInfoV2 {
            parent_slot: self.parent_slot,
            parent_blockhash: self.parent_blockhash,
            slot: self.slot,
            blockhash: self.blockhash,
            rewards: self.rewards,
            block_time: self.block_time,
            block_height: self.block_height,
            executed_transaction_count: self.executed_transaction_count,
        }
        .encode_ext(writer, dedupe.as_deref_mut())?;
        n += self.entry_count.encode_ext(writer, dedupe)?;
        Ok(n)
    }
}
impl Decode for ifc::ReplicaBlockInfoV3<'static> {
    #[inline]
    fn decode_ext(reader: &mut impl Read, mut dedupe: Option<&mut DedupeDecoder>) -> Result<Self> {
        let v2: ifc::ReplicaBlockInfoV2<'static> =
            Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let entry_count = Decode::decode_ext(reader, dedupe)?;
        Ok(Self {
            parent_slot: v2.parent_slot,
            parent_blockhash: v2.parent_blockhash,
            slot: v2.slot,
            blockhash: v2.blockhash,
            rewards: v2.rewards,
            block_time: v2.block_time,
            block_height: v2.block_height,
            executed_transaction_count: v2.executed_transaction_count,
            entry_count,
        })
    }
}
impl Encode for ifc::ReplicaBlockInfoV4<'_> {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        let mut n = 0;
        n += self.parent_slot.encode_ext(writer, dedupe.as_deref_mut())?;
        n += self
            .parent_blockhash
            .encode_ext(writer, dedupe.as_deref_mut())?;
        n += self.slot.encode_ext(writer, dedupe.as_deref_mut())?;
        n += self.blockhash.encode_ext(writer, dedupe.as_deref_mut())?;
        n += (*self.rewards).encode_ext(writer, dedupe.as_deref_mut())?;
        n += self.block_time.encode_ext(writer, dedupe.as_deref_mut())?;
        n += self
            .block_height
            .encode_ext(writer, dedupe.as_deref_mut())?;
        n += self
            .executed_transaction_count
            .encode_ext(writer, dedupe.as_deref_mut())?;
        n += self.entry_count.encode_ext(writer, dedupe)?;
        Ok(n)
    }
}
impl Decode for ifc::ReplicaBlockInfoV4<'static> {
    #[inline]
    fn decode_ext(reader: &mut impl Read, mut dedupe: Option<&mut DedupeDecoder>) -> Result<Self> {
        let parent_slot = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let parent_blockhash: String = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let slot = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let blockhash: String = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let rewards: txstatus3::RewardsAndNumPartitions =
            Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let block_time = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let block_height = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let executed_transaction_count = Decode::decode_ext(reader, dedupe.as_deref_mut())?;
        let entry_count = Decode::decode_ext(reader, dedupe)?;
        Ok(Self {
            parent_slot,
            parent_blockhash: Box::leak(parent_blockhash.into_boxed_str()),
            slot,
            blockhash: Box::leak(blockhash.into_boxed_str()),
            rewards: Box::leak(Box::new(rewards)),
            block_time,
            block_height,
            executed_transaction_count,
            entry_count,
        })
    }
}
impl Encode for ifc::ReplicaBlockInfoVersions<'_> {
    #[inline]
    fn encode_ext(
        &self,
        w: &mut impl Write,
        mut dedupe: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        match self {
            ifc::ReplicaBlockInfoVersions::V0_0_1(v) => {
                let mut n = <usize as Encode>::encode_discriminant(0, w)?;
                n += (*v).encode_ext(w, dedupe.as_deref_mut())?;
                Ok(n)
            }
            ifc::ReplicaBlockInfoVersions::V0_0_2(v) => {
                let mut n = <usize as Encode>::encode_discriminant(1, w)?;
                n += (*v).encode_ext(w, dedupe.as_deref_mut())?;
                Ok(n)
            }
            ifc::ReplicaBlockInfoVersions::V0_0_3(v) => {
                let mut n = <usize as Encode>::encode_discriminant(2, w)?;
                n += (*v).encode_ext(w, dedupe.as_deref_mut())?;
                Ok(n)
            }
            ifc::ReplicaBlockInfoVersions::V0_0_4(v) => {
                let mut n = <usize as Encode>::encode_discriminant(3, w)?;
                n += (*v).encode_ext(w, dedupe)?;
                Ok(n)
            }
        }
    }
}
impl Decode for ifc::ReplicaBlockInfoVersions<'static> {
    #[inline]
    fn decode_ext(reader: &mut impl Read, dedupe: Option<&mut DedupeDecoder>) -> Result<Self> {
        match <usize as Decode>::decode_discriminant(reader)? {
            0 => Ok(Self::V0_0_1(Box::leak(Box::new(Decode::decode_ext(
                reader, dedupe,
            )?)))),
            1 => Ok(Self::V0_0_2(Box::leak(Box::new(Decode::decode_ext(
                reader, dedupe,
            )?)))),
            2 => Ok(Self::V0_0_3(Box::leak(Box::new(Decode::decode_ext(
                reader, dedupe,
            )?)))),
            3 => Ok(Self::V0_0_4(Box::leak(Box::new(Decode::decode_ext(
                reader, dedupe,
            )?)))),
            _ => Err(Error::InvalidData),
        }
    }
}

// SlotStatus and GeyserPluginError
impl Encode for ifc::SlotStatus {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut _dedupe: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        match self {
            ifc::SlotStatus::Processed => <usize as Encode>::encode_discriminant(0, writer),
            ifc::SlotStatus::Rooted => <usize as Encode>::encode_discriminant(1, writer),
            ifc::SlotStatus::Confirmed => <usize as Encode>::encode_discriminant(2, writer),
            ifc::SlotStatus::FirstShredReceived => {
                <usize as Encode>::encode_discriminant(3, writer)
            }
            ifc::SlotStatus::Completed => <usize as Encode>::encode_discriminant(4, writer),
            ifc::SlotStatus::CreatedBank => <usize as Encode>::encode_discriminant(5, writer),
            ifc::SlotStatus::Dead(msg) => {
                let mut n = <usize as Encode>::encode_discriminant(6, writer)?;
                n += msg.encode_ext(writer, None)?;
                Ok(n)
            }
        }
    }
}
impl Decode for ifc::SlotStatus {
    #[inline]
    fn decode_ext(reader: &mut impl Read, _dedupe: Option<&mut DedupeDecoder>) -> Result<Self> {
        Ok(match <usize as Decode>::decode_discriminant(reader)? {
            0 => ifc::SlotStatus::Processed,
            1 => ifc::SlotStatus::Rooted,
            2 => ifc::SlotStatus::Confirmed,
            3 => ifc::SlotStatus::FirstShredReceived,
            4 => ifc::SlotStatus::Completed,
            5 => ifc::SlotStatus::CreatedBank,
            6 => ifc::SlotStatus::Dead(Decode::decode_ext(reader, None)?),
            _ => return Err(Error::InvalidData),
        })
    }
}

#[derive(Debug)]
struct SimpleError(String);
impl core::fmt::Display for SimpleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for SimpleError {}

impl Encode for ifc::GeyserPluginError {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        _dedupe: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        match self {
            ifc::GeyserPluginError::ConfigFileOpenError(e) => {
                let mut n = <usize as Encode>::encode_discriminant(0, writer)?;
                n += e.to_string().encode_ext(writer, None)?;
                Ok(n)
            }
            ifc::GeyserPluginError::ConfigFileReadError { msg } => {
                let mut n = <usize as Encode>::encode_discriminant(1, writer)?;
                n += msg.encode_ext(writer, None)?;
                Ok(n)
            }
            ifc::GeyserPluginError::AccountsUpdateError { msg } => {
                let mut n = <usize as Encode>::encode_discriminant(2, writer)?;
                n += msg.encode_ext(writer, None)?;
                Ok(n)
            }
            ifc::GeyserPluginError::SlotStatusUpdateError { msg } => {
                let mut n = <usize as Encode>::encode_discriminant(3, writer)?;
                n += msg.encode_ext(writer, None)?;
                Ok(n)
            }
            ifc::GeyserPluginError::Custom(err) => {
                let mut n = <usize as Encode>::encode_discriminant(4, writer)?;
                n += err.to_string().encode_ext(writer, None)?;
                Ok(n)
            }
            ifc::GeyserPluginError::TransactionUpdateError { msg } => {
                let mut n = <usize as Encode>::encode_discriminant(5, writer)?;
                n += msg.encode_ext(writer, None)?;
                Ok(n)
            }
        }
    }
}
impl Decode for ifc::GeyserPluginError {
    #[inline]
    fn decode_ext(reader: &mut impl Read, _dedupe: Option<&mut DedupeDecoder>) -> Result<Self> {
        Ok(match <usize as Decode>::decode_discriminant(reader)? {
            0 => ifc::GeyserPluginError::ConfigFileOpenError(std::io::Error::other(
                String::decode_ext(reader, None)?,
            )),
            1 => ifc::GeyserPluginError::ConfigFileReadError {
                msg: Decode::decode_ext(reader, None)?,
            },
            2 => ifc::GeyserPluginError::AccountsUpdateError {
                msg: Decode::decode_ext(reader, None)?,
            },
            3 => ifc::GeyserPluginError::SlotStatusUpdateError {
                msg: Decode::decode_ext(reader, None)?,
            },
            4 => ifc::GeyserPluginError::Custom(Box::new(SimpleError(Decode::decode_ext(
                reader, None,
            )?))),
            5 => ifc::GeyserPluginError::TransactionUpdateError {
                msg: Decode::decode_ext(reader, None)?,
            },
            _ => return Err(Error::InvalidData),
        })
    }
}

#[test]
fn test_agave_replica_transaction_info_versions_roundtrip() {
    use crate::prelude::*;

    // Build a minimal sanitized transaction (legacy) for v1/v2
    let header = msg3::MessageHeader {
        num_required_signatures: 1,
        num_readonly_signed_accounts: 0,
        num_readonly_unsigned_accounts: 1,
    };
    let account_keys = vec![pubkey3::Pubkey::new_unique(), pubkey3::Pubkey::new_unique()];
    let recent_blockhash = hash3::Hash::new_unique();
    let instructions = vec![msg3::compiled_instruction::CompiledInstruction {
        program_id_index: 1,
        accounts: vec![0],
        data: vec![1, 2, 3],
    }];
    let legacy = msg3::legacy::Message {
        header,
        account_keys,
        recent_blockhash,
        instructions,
    };
    let reserved = std::collections::HashSet::default();
    let legacy_msg = msg3::LegacyMessage::new(legacy, &reserved);
    let sanitized_msg = msg3::SanitizedMessage::Legacy(legacy_msg.clone());

    let signatures = vec![sig3::Signature::default()];
    let tx = tx3::sanitized::SanitizedTransaction::try_new_from_fields(
        sanitized_msg,
        hash3::Hash::new_unique(),
        false,
        signatures,
    )
    .unwrap();
    let meta = txstatus3::TransactionStatusMeta::default();

    // V0_0_1
    let info1 = ifc::ReplicaTransactionInfo {
        signature: &tx.signatures()[0],
        is_vote: false,
        transaction: &tx,
        transaction_status_meta: &meta,
    };
    let v1 = ifc::ReplicaTransactionInfoVersions::V0_0_1(&info1);
    let mut buf = Vec::new();
    v1.encode(&mut buf).unwrap();
    let d1: ifc::ReplicaTransactionInfoVersions<'static> = decode(&mut Cursor::new(&buf)).unwrap();
    match d1 {
        ifc::ReplicaTransactionInfoVersions::V0_0_1(di) => {
            assert_eq!(di.is_vote, false);
        }
        _ => panic!("wrong variant for V0_0_1"),
    }

    // V0_0_2
    let info2 = ifc::ReplicaTransactionInfoV2 {
        signature: &tx.signatures()[0],
        is_vote: false,
        transaction: &tx,
        transaction_status_meta: &meta,
        index: 7,
    };
    let v2 = ifc::ReplicaTransactionInfoVersions::V0_0_2(&info2);
    buf.clear();
    v2.encode(&mut buf).unwrap();
    let d2: ifc::ReplicaTransactionInfoVersions<'static> = decode(&mut Cursor::new(&buf)).unwrap();
    match d2 {
        ifc::ReplicaTransactionInfoVersions::V0_0_2(di) => {
            assert_eq!(di.index, 7);
        }
        _ => panic!("wrong variant for V0_0_2"),
    }

    // V0_0_3 requires a VersionedTransaction and message_hash
    let versioned = tx3::versioned::VersionedTransaction {
        signatures: tx.signatures().to_vec(),
        message: msg3::VersionedMessage::Legacy(legacy_msg.message.as_ref().clone()),
    };
    let mh = hash3::Hash::new_unique();
    let info3 = ifc::ReplicaTransactionInfoV3 {
        signature: &versioned.signatures[0],
        message_hash: &mh,
        is_vote: false,
        transaction: &versioned,
        transaction_status_meta: &meta,
        index: 9,
    };
    let v3 = ifc::ReplicaTransactionInfoVersions::V0_0_3(&info3);
    buf.clear();
    v3.encode(&mut buf).unwrap();
    let d3: ifc::ReplicaTransactionInfoVersions<'static> = decode(&mut Cursor::new(&buf)).unwrap();
    match d3 {
        ifc::ReplicaTransactionInfoVersions::V0_0_3(di) => {
            assert_eq!(di.index, 9);
            assert_eq!(di.is_vote, false);
        }
        _ => panic!("wrong variant for V0_0_3"),
    }
}

#[test]
fn test_agave_replica_account_info_versions_roundtrip() {
    use crate::prelude::*;
    let pubkey = [1u8; 32];
    let owner = [2u8; 32];
    let data = vec![3u8, 4, 5];
    let sig = sig3::Signature::default();

    // Base
    let base = ifc::ReplicaAccountInfo {
        pubkey: &pubkey,
        lamports: 123,
        owner: &owner,
        executable: false,
        rent_epoch: 99,
        data: &data,
        write_version: 42,
    };
    let v1 = ifc::ReplicaAccountInfoVersions::V0_0_1(&base);
    let mut buf = Vec::new();
    v1.encode(&mut buf).unwrap();
    let d1: ifc::ReplicaAccountInfoVersions<'static> = decode(&mut Cursor::new(&buf)).unwrap();
    match d1 {
        ifc::ReplicaAccountInfoVersions::V0_0_1(info) => {
            assert_eq!(info.lamports, 123);
            assert_eq!(info.pubkey, &pubkey);
            assert_eq!(info.data, &data[..]);
        }
        _ => panic!("wrong variant v1"),
    }

    // V2 with txn_signature
    let v2info = ifc::ReplicaAccountInfoV2 {
        pubkey: base.pubkey,
        lamports: base.lamports,
        owner: base.owner,
        executable: base.executable,
        rent_epoch: base.rent_epoch,
        data: base.data,
        write_version: base.write_version,
        txn_signature: Some(&sig),
    };
    let v2 = ifc::ReplicaAccountInfoVersions::V0_0_2(&v2info);
    buf.clear();
    v2.encode(&mut buf).unwrap();
    let d2: ifc::ReplicaAccountInfoVersions<'static> = decode(&mut Cursor::new(&buf)).unwrap();
    match d2 {
        ifc::ReplicaAccountInfoVersions::V0_0_2(info) => {
            assert!(info.txn_signature.is_some());
        }
        _ => panic!("wrong variant v2"),
    }

    // V3 with txn
    // Build a tiny sanitized tx for reference
    let header = msg3::MessageHeader {
        num_required_signatures: 1,
        num_readonly_signed_accounts: 0,
        num_readonly_unsigned_accounts: 1,
    };
    let account_keys = vec![pubkey3::Pubkey::new_unique(), pubkey3::Pubkey::new_unique()];
    let recent_blockhash = hash3::Hash::new_unique();
    let instructions = vec![msg3::compiled_instruction::CompiledInstruction {
        program_id_index: 1,
        accounts: vec![0],
        data: vec![],
    }];
    let legacy = msg3::legacy::Message {
        header,
        account_keys,
        recent_blockhash,
        instructions,
    };
    let reserved = std::collections::HashSet::default();
    let legacy_msg = msg3::LegacyMessage::new(legacy, &reserved);
    let sanitized_msg = msg3::SanitizedMessage::Legacy(legacy_msg);
    let tx = tx3::sanitized::SanitizedTransaction::try_new_from_fields(
        sanitized_msg,
        hash3::Hash::new_unique(),
        false,
        vec![sig3::Signature::default()],
    )
    .unwrap();

    let v3info = ifc::ReplicaAccountInfoV3 {
        pubkey: base.pubkey,
        lamports: base.lamports,
        owner: base.owner,
        executable: base.executable,
        rent_epoch: base.rent_epoch,
        data: base.data,
        write_version: base.write_version,
        txn: Some(&tx),
    };
    let v3 = ifc::ReplicaAccountInfoVersions::V0_0_3(&v3info);
    buf.clear();
    v3.encode(&mut buf).unwrap();
    let d3: ifc::ReplicaAccountInfoVersions<'static> = decode(&mut Cursor::new(&buf)).unwrap();
    match d3 {
        ifc::ReplicaAccountInfoVersions::V0_0_3(info) => {
            assert!(info.txn.is_some());
            assert_eq!(info.write_version, 42);
        }
        _ => panic!("wrong variant v3"),
    }
}

#[test]
fn test_agave_replica_entry_info_versions_roundtrip() {
    use crate::prelude::*;
    let hash = vec![9u8, 8, 7, 6];
    let e1 = ifc::ReplicaEntryInfo {
        slot: 10,
        index: 2,
        num_hashes: 5,
        hash: &hash,
        executed_transaction_count: 3,
    };
    let v1 = ifc::ReplicaEntryInfoVersions::V0_0_1(&e1);
    let mut buf = Vec::new();
    v1.encode(&mut buf).unwrap();
    let d1: ifc::ReplicaEntryInfoVersions<'static> = decode(&mut Cursor::new(&buf)).unwrap();
    match d1 {
        ifc::ReplicaEntryInfoVersions::V0_0_1(x) => assert_eq!(x.slot, 10),
        _ => panic!(),
    }

    let e2 = ifc::ReplicaEntryInfoV2 {
        slot: e1.slot,
        index: e1.index,
        num_hashes: e1.num_hashes,
        hash: e1.hash,
        executed_transaction_count: e1.executed_transaction_count,
        starting_transaction_index: 77,
    };
    let v2 = ifc::ReplicaEntryInfoVersions::V0_0_2(&e2);
    buf.clear();
    v2.encode(&mut buf).unwrap();
    let d2: ifc::ReplicaEntryInfoVersions<'static> = decode(&mut Cursor::new(&buf)).unwrap();
    match d2 {
        ifc::ReplicaEntryInfoVersions::V0_0_2(x) => assert_eq!(x.starting_transaction_index, 77),
        _ => panic!(),
    }
}

#[test]
fn test_agave_replica_block_info_versions_roundtrip() {
    use crate::prelude::*;
    let rewards = vec![txstatus3::Reward {
        pubkey: "pk".into(),
        lamports: 1,
        post_balance: 2,
        reward_type: Some(reward_info::RewardType::Fee),
        commission: Some(1),
    }];
    let blockhash = String::from("bh");
    let r1 = ifc::ReplicaBlockInfo {
        slot: 5,
        blockhash: &blockhash,
        rewards: &rewards,
        block_time: Some(123),
        block_height: Some(7),
    };
    let v1 = ifc::ReplicaBlockInfoVersions::V0_0_1(&r1);
    let mut buf = Vec::new();
    v1.encode(&mut buf).unwrap();
    let d1: ifc::ReplicaBlockInfoVersions<'static> = decode(&mut Cursor::new(&buf)).unwrap();
    match d1 {
        ifc::ReplicaBlockInfoVersions::V0_0_1(b) => {
            assert_eq!(b.slot, 5);
            assert_eq!(b.blockhash, "bh");
        }
        _ => panic!(),
    }

    let parent_blockhash = String::from("pbh");
    let r2 = ifc::ReplicaBlockInfoV2 {
        parent_slot: 4,
        parent_blockhash: &parent_blockhash,
        slot: 6,
        blockhash: &blockhash,
        rewards: &rewards,
        block_time: Some(321),
        block_height: Some(8),
        executed_transaction_count: 11,
    };
    let v2 = ifc::ReplicaBlockInfoVersions::V0_0_2(&r2);
    buf.clear();
    v2.encode(&mut buf).unwrap();
    let d2: ifc::ReplicaBlockInfoVersions<'static> = decode(&mut Cursor::new(&buf)).unwrap();
    match d2 {
        ifc::ReplicaBlockInfoVersions::V0_0_2(b) => {
            assert_eq!(b.parent_slot, 4);
            assert_eq!(b.executed_transaction_count, 11);
        }
        _ => panic!(),
    }

    let r3 = ifc::ReplicaBlockInfoV3 {
        parent_slot: r2.parent_slot,
        parent_blockhash: r2.parent_blockhash,
        slot: r2.slot,
        blockhash: r2.blockhash,
        rewards: r2.rewards,
        block_time: r2.block_time,
        block_height: r2.block_height,
        executed_transaction_count: r2.executed_transaction_count,
        entry_count: 99,
    };
    let v3 = ifc::ReplicaBlockInfoVersions::V0_0_3(&r3);
    buf.clear();
    v3.encode(&mut buf).unwrap();
    let d3: ifc::ReplicaBlockInfoVersions<'static> = decode(&mut Cursor::new(&buf)).unwrap();
    match d3 {
        ifc::ReplicaBlockInfoVersions::V0_0_3(b) => assert_eq!(b.entry_count, 99),
        _ => panic!(),
    }

    let rap = txstatus3::RewardsAndNumPartitions {
        rewards: rewards.clone(),
        num_partitions: Some(2),
    };
    let r4 = ifc::ReplicaBlockInfoV4 {
        parent_slot: 3,
        parent_blockhash: &parent_blockhash,
        slot: 7,
        blockhash: &blockhash,
        rewards: &rap,
        block_time: None,
        block_height: None,
        executed_transaction_count: 1,
        entry_count: 2,
    };
    let v4 = ifc::ReplicaBlockInfoVersions::V0_0_4(&r4);
    buf.clear();
    v4.encode(&mut buf).unwrap();
    let d4: ifc::ReplicaBlockInfoVersions<'static> = decode(&mut Cursor::new(&buf)).unwrap();
    match d4 {
        ifc::ReplicaBlockInfoVersions::V0_0_4(b) => {
            assert_eq!(b.rewards.num_partitions, Some(2));
            assert_eq!(b.entry_count, 2);
        }
        _ => panic!(),
    }
}

#[test]
fn test_agave_slot_status_roundtrip() {
    use crate::prelude::*;
    let variants = [
        ifc::SlotStatus::Processed,
        ifc::SlotStatus::Rooted,
        ifc::SlotStatus::Confirmed,
        ifc::SlotStatus::FirstShredReceived,
        ifc::SlotStatus::Completed,
        ifc::SlotStatus::CreatedBank,
        ifc::SlotStatus::Dead("oops".into()),
    ];
    for v in variants {
        let mut buf = Vec::new();
        v.encode(&mut buf).unwrap();
        let d: ifc::SlotStatus = decode(&mut Cursor::new(&buf)).unwrap();
        match (&v, &d) {
            (ifc::SlotStatus::Dead(a), ifc::SlotStatus::Dead(b)) => assert_eq!(a, b),
            (a, b) => assert_eq!(a.as_str(), b.as_str()),
        }
    }
}

#[test]
fn test_agave_geyser_plugin_error_roundtrip() {
    use crate::prelude::*;
    let errs = vec![
        ifc::GeyserPluginError::ConfigFileReadError { msg: "bad".into() },
        ifc::GeyserPluginError::AccountsUpdateError { msg: "acc".into() },
        ifc::GeyserPluginError::SlotStatusUpdateError { msg: "slot".into() },
        ifc::GeyserPluginError::Custom(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "custom",
        ))),
        ifc::GeyserPluginError::TransactionUpdateError { msg: "tx".into() },
    ];
    for e in errs {
        let mut buf = Vec::new();
        e.encode(&mut buf).unwrap();
        let d: ifc::GeyserPluginError = decode(&mut Cursor::new(&buf)).unwrap();
        match (e, d) {
            (
                ifc::GeyserPluginError::ConfigFileReadError { msg: a },
                ifc::GeyserPluginError::ConfigFileReadError { msg: b },
            ) => assert_eq!(a, b),
            (
                ifc::GeyserPluginError::AccountsUpdateError { msg: a },
                ifc::GeyserPluginError::AccountsUpdateError { msg: b },
            ) => assert_eq!(a, b),
            (
                ifc::GeyserPluginError::SlotStatusUpdateError { msg: a },
                ifc::GeyserPluginError::SlotStatusUpdateError { msg: b },
            ) => assert_eq!(a, b),
            (
                ifc::GeyserPluginError::TransactionUpdateError { msg: a },
                ifc::GeyserPluginError::TransactionUpdateError { msg: b },
            ) => assert_eq!(a, b),
            (ifc::GeyserPluginError::Custom(a), ifc::GeyserPluginError::Custom(b)) => {
                assert_eq!(a.to_string(), b.to_string())
            }
            (left, right) => panic!("mismatch: {left:?} vs {right:?}"),
        }
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
                total += message.encode_ext(writer, dedupe_encoder)?;
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
                let v0msg = v0::Message::decode_ext(reader, dedupe_decoder)?;
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
        total += self.message.encode_ext(writer, dedupe_encoder)?;
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
        let message = VersionedMessage::decode_ext(reader, dedupe_decoder)?;
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
