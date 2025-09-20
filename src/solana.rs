use agave_geyser_plugin_interface::geyser_plugin_interface as ifc;
use solana_account_decoder_client_types as acct_dec_client;
use solana_hash as hash3;
use solana_instruction::error as ixerr;
use solana_message as msg3;
use solana_pubkey as pubkey3;
use solana_reward_info as reward_info;
use solana_signature as sig3;
use solana_transaction as tx3;
use solana_transaction_context as txctx3;
use solana_transaction_error as txerr3;
use solana_transaction_status as txstatus3;

use crate::prelude::*;

#[cfg(test)]
use hash3::Hash;
#[cfg(test)]
use msg3::{
    LegacyMessage, Message, MessageHeader, SanitizedMessage,
    compiled_instruction::CompiledInstruction,
    v0::{self, MessageAddressTableLookup},
};
#[cfg(test)]
use pubkey3::Pubkey;
#[cfg(test)]
use sig3::Signature;
#[cfg(test)]
use tx3::versioned::VersionedTransaction;

// Implementations for Agave (v3) Geyser interface and its dependencies (inline)

// No serde/bincode usage in this module; all types implement Encode/Decode directly.

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
// InstructionError encoding (direct, no serde)
impl Encode for ixerr::InstructionError {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        _dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        use ixerr::InstructionError as E;
        let disc: usize = match self {
            E::GenericError => 0,
            E::InvalidArgument => 1,
            E::InvalidInstructionData => 2,
            E::InvalidAccountData => 3,
            E::AccountDataTooSmall => 4,
            E::InsufficientFunds => 5,
            E::IncorrectProgramId => 6,
            E::MissingRequiredSignature => 7,
            E::AccountAlreadyInitialized => 8,
            E::UninitializedAccount => 9,
            E::UnbalancedInstruction => 10,
            E::ModifiedProgramId => 11,
            E::ExternalAccountLamportSpend => 12,
            E::ExternalAccountDataModified => 13,
            E::ReadonlyLamportChange => 14,
            E::ReadonlyDataModified => 15,
            E::DuplicateAccountIndex => 16,
            E::ExecutableModified => 17,
            E::RentEpochModified => 18,
            E::NotEnoughAccountKeys => 19,
            E::AccountDataSizeChanged => 20,
            E::AccountNotExecutable => 21,
            E::AccountBorrowFailed => 22,
            E::AccountBorrowOutstanding => 23,
            E::DuplicateAccountOutOfSync => 24,
            E::Custom(_) => 25,
            E::InvalidError => 26,
            E::ExecutableDataModified => 27,
            E::ExecutableLamportChange => 28,
            E::ExecutableAccountNotRentExempt => 29,
            E::UnsupportedProgramId => 30,
            E::CallDepth => 31,
            E::MissingAccount => 32,
            E::ReentrancyNotAllowed => 33,
            E::MaxSeedLengthExceeded => 34,
            E::InvalidSeeds => 35,
            E::InvalidRealloc => 36,
            E::ComputationalBudgetExceeded => 37,
            E::PrivilegeEscalation => 38,
            E::ProgramEnvironmentSetupFailure => 39,
            E::ProgramFailedToComplete => 40,
            E::ProgramFailedToCompile => 41,
            E::Immutable => 42,
            E::IncorrectAuthority => 43,
            E::BorshIoError => 44,
            E::AccountNotRentExempt => 45,
            E::InvalidAccountOwner => 46,
            E::ArithmeticOverflow => 47,
            E::UnsupportedSysvar => 48,
            E::IllegalOwner => 49,
            E::MaxAccountsDataAllocationsExceeded => 50,
            E::MaxAccountsExceeded => 51,
            E::MaxInstructionTraceLengthExceeded => 52,
            E::BuiltinProgramsMustConsumeComputeUnits => 53,
        };
        let mut n = <usize as Encode>::encode_discriminant(disc, writer)?;
        if let E::Custom(code) = self {
            n += code.encode_ext(writer, None)?;
        }
        Ok(n)
    }
}

impl Decode for ixerr::InstructionError {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        _dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        use ixerr::InstructionError as E;
        Ok(match <usize as Decode>::decode_discriminant(reader)? {
            0 => E::GenericError,
            1 => E::InvalidArgument,
            2 => E::InvalidInstructionData,
            3 => E::InvalidAccountData,
            4 => E::AccountDataTooSmall,
            5 => E::InsufficientFunds,
            6 => E::IncorrectProgramId,
            7 => E::MissingRequiredSignature,
            8 => E::AccountAlreadyInitialized,
            9 => E::UninitializedAccount,
            10 => E::UnbalancedInstruction,
            11 => E::ModifiedProgramId,
            12 => E::ExternalAccountLamportSpend,
            13 => E::ExternalAccountDataModified,
            14 => E::ReadonlyLamportChange,
            15 => E::ReadonlyDataModified,
            16 => E::DuplicateAccountIndex,
            17 => E::ExecutableModified,
            18 => E::RentEpochModified,
            19 => E::NotEnoughAccountKeys,
            20 => E::AccountDataSizeChanged,
            21 => E::AccountNotExecutable,
            22 => E::AccountBorrowFailed,
            23 => E::AccountBorrowOutstanding,
            24 => E::DuplicateAccountOutOfSync,
            25 => E::Custom(Decode::decode_ext(reader, None)?),
            26 => E::InvalidError,
            27 => E::ExecutableDataModified,
            28 => E::ExecutableLamportChange,
            29 => E::ExecutableAccountNotRentExempt,
            30 => E::UnsupportedProgramId,
            31 => E::CallDepth,
            32 => E::MissingAccount,
            33 => E::ReentrancyNotAllowed,
            34 => E::MaxSeedLengthExceeded,
            35 => E::InvalidSeeds,
            36 => E::InvalidRealloc,
            37 => E::ComputationalBudgetExceeded,
            38 => E::PrivilegeEscalation,
            39 => E::ProgramEnvironmentSetupFailure,
            40 => E::ProgramFailedToComplete,
            41 => E::ProgramFailedToCompile,
            42 => E::Immutable,
            43 => E::IncorrectAuthority,
            44 => E::BorshIoError,
            45 => E::AccountNotRentExempt,
            46 => E::InvalidAccountOwner,
            47 => E::ArithmeticOverflow,
            48 => E::UnsupportedSysvar,
            49 => E::IllegalOwner,
            50 => E::MaxAccountsDataAllocationsExceeded,
            51 => E::MaxAccountsExceeded,
            52 => E::MaxInstructionTraceLengthExceeded,
            53 => E::BuiltinProgramsMustConsumeComputeUnits,
            _ => return Err(Error::InvalidData),
        })
    }
}

// TransactionError encoding (direct, no serde)
impl Encode for txerr3::TransactionError {
    #[inline]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        _dedupe_encoder: Option<&mut DedupeEncoder>,
    ) -> Result<usize> {
        use txerr3::TransactionError as E;
        let disc: usize = match self {
            E::AccountInUse => 0,
            E::AccountLoadedTwice => 1,
            E::AccountNotFound => 2,
            E::ProgramAccountNotFound => 3,
            E::InsufficientFundsForFee => 4,
            E::InvalidAccountForFee => 5,
            E::AlreadyProcessed => 6,
            E::BlockhashNotFound => 7,
            E::InstructionError(_, _) => 8,
            E::CallChainTooDeep => 9,
            E::MissingSignatureForFee => 10,
            E::InvalidAccountIndex => 11,
            E::SignatureFailure => 12,
            E::InvalidProgramForExecution => 13,
            E::SanitizeFailure => 14,
            E::ClusterMaintenance => 15,
            E::AccountBorrowOutstanding => 16,
            E::WouldExceedMaxBlockCostLimit => 17,
            E::UnsupportedVersion => 18,
            E::InvalidWritableAccount => 19,
            E::WouldExceedMaxAccountCostLimit => 20,
            E::WouldExceedAccountDataBlockLimit => 21,
            E::TooManyAccountLocks => 22,
            E::AddressLookupTableNotFound => 23,
            E::InvalidAddressLookupTableOwner => 24,
            E::InvalidAddressLookupTableData => 25,
            E::InvalidAddressLookupTableIndex => 26,
            E::InvalidRentPayingAccount => 27,
            E::WouldExceedMaxVoteCostLimit => 28,
            E::WouldExceedAccountDataTotalLimit => 29,
            E::DuplicateInstruction(_) => 30,
            E::InsufficientFundsForRent { .. } => 31,
            E::MaxLoadedAccountsDataSizeExceeded => 32,
            E::InvalidLoadedAccountsDataSizeLimit => 33,
            E::ResanitizationNeeded => 34,
            E::ProgramExecutionTemporarilyRestricted { .. } => 35,
            E::UnbalancedTransaction => 36,
            E::ProgramCacheHitMaxLimit => 37,
            E::CommitCancelled => 38,
        };
        let mut n = <usize as Encode>::encode_discriminant(disc, writer)?;
        match self {
            E::InstructionError(idx, err) => {
                n += idx.encode_ext(writer, None)?;
                n += err.encode_ext(writer, None)?;
            }
            E::DuplicateInstruction(idx) => {
                n += idx.encode_ext(writer, None)?;
            }
            E::InsufficientFundsForRent { account_index } => {
                n += account_index.encode_ext(writer, None)?;
            }
            E::ProgramExecutionTemporarilyRestricted { account_index } => {
                n += account_index.encode_ext(writer, None)?;
            }
            _ => {}
        }
        Ok(n)
    }
}

impl Decode for txerr3::TransactionError {
    #[inline]
    fn decode_ext(
        reader: &mut impl Read,
        _dedupe_decoder: Option<&mut DedupeDecoder>,
    ) -> Result<Self> {
        use txerr3::TransactionError as E;
        Ok(match <usize as Decode>::decode_discriminant(reader)? {
            0 => E::AccountInUse,
            1 => E::AccountLoadedTwice,
            2 => E::AccountNotFound,
            3 => E::ProgramAccountNotFound,
            4 => E::InsufficientFundsForFee,
            5 => E::InvalidAccountForFee,
            6 => E::AlreadyProcessed,
            7 => E::BlockhashNotFound,
            8 => E::InstructionError(
                Decode::decode_ext(reader, None)?,
                Decode::decode_ext(reader, None)?,
            ),
            9 => E::CallChainTooDeep,
            10 => E::MissingSignatureForFee,
            11 => E::InvalidAccountIndex,
            12 => E::SignatureFailure,
            13 => E::InvalidProgramForExecution,
            14 => E::SanitizeFailure,
            15 => E::ClusterMaintenance,
            16 => E::AccountBorrowOutstanding,
            17 => E::WouldExceedMaxBlockCostLimit,
            18 => E::UnsupportedVersion,
            19 => E::InvalidWritableAccount,
            20 => E::WouldExceedMaxAccountCostLimit,
            21 => E::WouldExceedAccountDataBlockLimit,
            22 => E::TooManyAccountLocks,
            23 => E::AddressLookupTableNotFound,
            24 => E::InvalidAddressLookupTableOwner,
            25 => E::InvalidAddressLookupTableData,
            26 => E::InvalidAddressLookupTableIndex,
            27 => E::InvalidRentPayingAccount,
            28 => E::WouldExceedMaxVoteCostLimit,
            29 => E::WouldExceedAccountDataTotalLimit,
            30 => E::DuplicateInstruction(Decode::decode_ext(reader, None)?),
            31 => E::InsufficientFundsForRent {
                account_index: Decode::decode_ext(reader, None)?,
            },
            32 => E::MaxLoadedAccountsDataSizeExceeded,
            33 => E::InvalidLoadedAccountsDataSizeLimit,
            34 => E::ResanitizationNeeded,
            35 => E::ProgramExecutionTemporarilyRestricted {
                account_index: Decode::decode_ext(reader, None)?,
            },
            36 => E::UnbalancedTransaction,
            37 => E::ProgramCacheHitMaxLimit,
            38 => E::CommitCancelled,
            _ => return Err(Error::InvalidData),
        })
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
// Note: We intentionally do not implement Encode/Decode for agave-geyser
// interface wrappers that carry reference fields, to avoid requiring leaked
// allocations for decoding. These values can be reconstructed from their
// underlying owned types when needed.

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
// ===== Tests for Solana (v2) and Agave (v3) types =====

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
    let vm = msg3::VersionedMessage::Legacy(legacy);

    let mut buf = Vec::new();
    vm.encode(&mut buf).unwrap();
    let decoded = msg3::VersionedMessage::decode(&mut std::io::Cursor::new(&buf)).unwrap();
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
    let vm = msg3::VersionedMessage::V0(v0msg);

    let mut buf = Vec::new();
    vm.encode(&mut buf).unwrap();
    let decoded = msg3::VersionedMessage::decode(&mut std::io::Cursor::new(&buf)).unwrap();
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
    let message = msg3::VersionedMessage::Legacy(Message {
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
    let tx_dec = tx3::versioned::VersionedTransaction::decode_ext(
        &mut std::io::Cursor::new(&buf_dedupe),
        Some(&mut dec),
    )
    .unwrap();
    assert_eq!(tx, tx_dec);
}

// ---- Agave (v3) message primitives ----

#[test]
fn test_msg3_message_header_roundtrip() {
    use crate::prelude::*;
    let header = msg3::MessageHeader {
        num_required_signatures: 2,
        num_readonly_signed_accounts: 1,
        num_readonly_unsigned_accounts: 3,
    };
    let mut buf = Vec::new();
    header.encode(&mut buf).unwrap();
    let decoded: msg3::MessageHeader = decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(header, decoded);
}

#[test]
fn test_msg3_compiled_instruction_roundtrip() {
    use crate::prelude::*;
    let ci = msg3::compiled_instruction::CompiledInstruction {
        program_id_index: 7,
        accounts: vec![0, 2, 4],
        data: vec![1, 2, 3, 5, 8],
    };
    let mut buf = Vec::new();
    ci.encode(&mut buf).unwrap();
    let decoded: msg3::compiled_instruction::CompiledInstruction =
        decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(ci, decoded);
}

#[test]
fn test_msg3_legacy_message_roundtrip() {
    use crate::prelude::*;
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
        data: vec![9, 9, 9],
    }];
    let msg = msg3::legacy::Message {
        header,
        account_keys,
        recent_blockhash,
        instructions,
    };
    let mut buf = Vec::new();
    msg.encode(&mut buf).unwrap();
    let decoded: msg3::legacy::Message = decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(msg, decoded);
}

#[test]
fn test_msg3_v0_lookup_and_message_roundtrip() {
    use crate::prelude::*;
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
    let lookup = msg3::v0::MessageAddressTableLookup {
        account_key: pubkey3::Pubkey::new_unique(),
        writable_indexes: vec![0, 2],
        readonly_indexes: vec![1],
    };
    let v0msg = msg3::v0::Message {
        header,
        account_keys,
        recent_blockhash,
        instructions,
        address_table_lookups: vec![lookup],
    };

    // Lookup alone
    let mut buf = Vec::new();
    v0msg.address_table_lookups[0].encode(&mut buf).unwrap();
    let dec_lookup: msg3::v0::MessageAddressTableLookup = decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(v0msg.address_table_lookups[0], dec_lookup);

    // Entire v0 message
    buf.clear();
    v0msg.encode(&mut buf).unwrap();
    let decoded: msg3::v0::Message = decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(v0msg, decoded);
}

#[test]
fn test_msg3_sanitized_message_roundtrip_both_variants() {
    use crate::prelude::*;
    // Legacy variant
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
        data: vec![1],
    }];
    let legacy = msg3::legacy::Message {
        header,
        account_keys,
        recent_blockhash,
        instructions,
    };
    let reserved = std::collections::HashSet::default();
    let legacy_msg = msg3::LegacyMessage::new(legacy, &reserved);
    let s_legacy = msg3::SanitizedMessage::Legacy(legacy_msg);

    let mut buf = Vec::new();
    s_legacy.encode(&mut buf).unwrap();
    let dec_legacy: msg3::SanitizedMessage = decode(&mut Cursor::new(&buf)).unwrap();
    match dec_legacy {
        msg3::SanitizedMessage::Legacy(_) => {}
        _ => panic!("wrong variant"),
    }

    // V0 variant with loaded addresses
    let v0msg = msg3::v0::Message {
        header,
        account_keys: vec![pubkey3::Pubkey::new_unique(), pubkey3::Pubkey::new_unique()],
        recent_blockhash: hash3::Hash::new_unique(),
        instructions: vec![],
        address_table_lookups: vec![],
    };
    let addrs = msg3::v0::LoadedAddresses {
        writable: vec![pubkey3::Pubkey::new_unique()],
        readonly: vec![pubkey3::Pubkey::new_unique()],
    };
    let loaded = msg3::v0::LoadedMessage::new(v0msg, addrs, &reserved);
    let s_v0 = msg3::SanitizedMessage::V0(loaded);
    buf.clear();
    s_v0.encode(&mut buf).unwrap();
    let dec_v0: msg3::SanitizedMessage = decode(&mut Cursor::new(&buf)).unwrap();
    match dec_v0 {
        msg3::SanitizedMessage::V0(_) => {}
        _ => panic!("wrong variant"),
    }
}

#[test]
fn test_msg3_loaded_addresses_and_message_roundtrip() {
    use crate::prelude::*;
    let addrs = msg3::v0::LoadedAddresses {
        writable: vec![pubkey3::Pubkey::new_unique(), pubkey3::Pubkey::new_unique()],
        readonly: vec![pubkey3::Pubkey::new_unique()],
    };
    let mut buf = Vec::new();
    addrs.encode(&mut buf).unwrap();
    let dec_addrs: msg3::v0::LoadedAddresses = decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(addrs, dec_addrs);
}

#[test]
fn test_tx3_sanitized_transaction_roundtrips() {
    use crate::prelude::*;
    // Legacy
    let header = msg3::MessageHeader {
        num_required_signatures: 1,
        num_readonly_signed_accounts: 0,
        num_readonly_unsigned_accounts: 1,
    };
    let legacy_msg = {
        let account_keys = vec![pubkey3::Pubkey::new_unique(), pubkey3::Pubkey::new_unique()];
        let recent_blockhash = hash3::Hash::new_unique();
        let instructions = vec![msg3::compiled_instruction::CompiledInstruction {
            program_id_index: 1,
            accounts: vec![0],
            data: vec![1, 2],
        }];
        let legacy = msg3::legacy::Message {
            header,
            account_keys,
            recent_blockhash,
            instructions,
        };
        let reserved = std::collections::HashSet::default();
        msg3::LegacyMessage::new(legacy, &reserved)
    };
    let s_legacy = msg3::SanitizedMessage::Legacy(legacy_msg);
    let tx_legacy = tx3::sanitized::SanitizedTransaction::try_new_from_fields(
        s_legacy,
        hash3::Hash::new_unique(),
        false,
        vec![sig3::Signature::default()],
    )
    .unwrap();
    let mut buf = Vec::new();
    tx_legacy.encode(&mut buf).unwrap();
    let dec_legacy = tx3::sanitized::SanitizedTransaction::decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(tx_legacy, dec_legacy);

    // V0
    let v0msg = msg3::v0::Message {
        header,
        account_keys: vec![pubkey3::Pubkey::new_unique(), pubkey3::Pubkey::new_unique()],
        recent_blockhash: hash3::Hash::new_unique(),
        instructions: vec![],
        address_table_lookups: vec![],
    };
    let loaded = msg3::v0::LoadedMessage::new(
        v0msg,
        msg3::v0::LoadedAddresses {
            writable: vec![],
            readonly: vec![],
        },
        &std::collections::HashSet::default(),
    );
    let s_v0 = msg3::SanitizedMessage::V0(loaded);
    let tx_v0 = tx3::sanitized::SanitizedTransaction::try_new_from_fields(
        s_v0,
        hash3::Hash::new_unique(),
        false,
        vec![sig3::Signature::default()],
    )
    .unwrap();
    buf.clear();
    tx_v0.encode(&mut buf).unwrap();
    let dec_v0 = tx3::sanitized::SanitizedTransaction::decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(tx_v0, dec_v0);
}

#[test]
fn test_tx3_versioned_transaction_roundtrip_and_dedupe() {
    use crate::prelude::*;
    let header = msg3::MessageHeader {
        num_required_signatures: 1,
        num_readonly_signed_accounts: 0,
        num_readonly_unsigned_accounts: 2,
    };
    let k = pubkey3::Pubkey::new_unique();
    let message = msg3::VersionedMessage::Legacy(msg3::legacy::Message {
        header,
        account_keys: vec![k, k, k], // duplicates to benefit dedupe
        recent_blockhash: hash3::Hash::new_unique(),
        instructions: vec![msg3::compiled_instruction::CompiledInstruction {
            program_id_index: 2,
            accounts: vec![0, 1],
            data: vec![0xEE],
        }],
    });
    let tx = tx3::versioned::VersionedTransaction {
        signatures: vec![sig3::Signature::default()],
        message,
    };

    let mut buf_plain = Vec::new();
    tx.encode_ext(&mut buf_plain, None).unwrap();
    let mut enc = DedupeEncoder::new();
    let mut buf_dedupe = Vec::new();
    tx.encode_ext(&mut buf_dedupe, Some(&mut enc)).unwrap();
    assert!(buf_dedupe.len() < buf_plain.len());
    let mut dec = DedupeDecoder::new();
    let rt = tx3::versioned::VersionedTransaction::decode_ext(
        &mut Cursor::new(&buf_dedupe),
        Some(&mut dec),
    )
    .unwrap();
    assert_eq!(tx, rt);
}

// ---- Selected client/status types ----

#[test]
fn test_ui_token_amount_roundtrip() {
    use crate::prelude::*;
    let v = acct_dec_client::token::UiTokenAmount {
        ui_amount: Some(42.5),
        decimals: 6,
        amount: "42500000".into(),
        ui_amount_string: "42.5".into(),
    };
    let mut buf = Vec::new();
    v.encode(&mut buf).unwrap();
    let d: acct_dec_client::token::UiTokenAmount = decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(v, d);
}

#[test]
fn test_rewards_and_partitions_roundtrip() {
    use crate::prelude::*;
    let r = txstatus3::Reward {
        pubkey: "pk".into(),
        lamports: 1,
        post_balance: 2,
        reward_type: Some(reward_info::RewardType::Fee),
        commission: Some(3),
    };
    let rap = txstatus3::RewardsAndNumPartitions {
        rewards: vec![r],
        num_partitions: Some(2),
    };
    let mut buf = Vec::new();
    rap.encode(&mut buf).unwrap();
    let d: txstatus3::RewardsAndNumPartitions = decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(rap, d);
}

#[test]
fn test_txctx_return_data_roundtrip() {
    use crate::prelude::*;
    let v = txctx3::TransactionReturnData {
        program_id: pubkey3::Pubkey::new_unique(),
        data: vec![1, 2, 3],
    };
    let mut buf = Vec::new();
    v.encode(&mut buf).unwrap();
    let d: txctx3::TransactionReturnData = decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(v, d);
}

#[test]
fn test_txstatus_meta_default_roundtrip() {
    use crate::prelude::*;
    let meta = txstatus3::TransactionStatusMeta::default();
    let mut buf = Vec::new();
    meta.encode(&mut buf).unwrap();
    let d: txstatus3::TransactionStatusMeta = decode(&mut Cursor::new(&buf)).unwrap();
    assert_eq!(meta, d);
}

#[test]
fn test_transaction_error_roundtrip() {
    use crate::prelude::*;
    let cases = vec![
        txerr3::TransactionError::AccountInUse,
        txerr3::TransactionError::InstructionError(5, ixerr::InstructionError::Custom(42)),
        txerr3::TransactionError::DuplicateInstruction(9),
        txerr3::TransactionError::InsufficientFundsForRent { account_index: 7 },
        txerr3::TransactionError::ProgramExecutionTemporarilyRestricted { account_index: 3 },
    ];
    for e in cases {
        let mut buf = Vec::new();
        e.encode(&mut buf).unwrap();
        let d: txerr3::TransactionError = decode(&mut Cursor::new(&buf)).unwrap();
        assert_eq!(e, d);
    }
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
    let original = msg3::SanitizedMessage::Legacy(legacy_message);

    let mut buffer = Vec::new();
    let bytes_written = original.encode(&mut buffer).unwrap();
    assert!(bytes_written > 0);

    let mut cursor = Cursor::new(&buffer);
    let decoded: SanitizedMessage = msg3::SanitizedMessage::decode(&mut cursor).unwrap();

    match (&original, &decoded) {
        (msg3::SanitizedMessage::Legacy(orig), msg3::SanitizedMessage::Legacy(decoded)) => {
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

    let sanitized = msg3::SanitizedMessage::Legacy(legacy_message);
    let signatures = vec![Signature::default(), Signature::default()];
    let tx = tx3::sanitized::SanitizedTransaction::try_new_from_fields(
        sanitized,
        Hash::new_unique(),
        false,
        signatures,
    )
    .unwrap();

    // Round-trip encode/decode
    let mut buf = Vec::new();
    tx.encode(&mut buf).unwrap();
    let decoded = tx3::sanitized::SanitizedTransaction::decode(&mut Cursor::new(&buf)).unwrap();
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
    let sanitized = msg3::SanitizedMessage::V0(sanitized_v0);

    let signatures = vec![Signature::default()];
    let tx = tx3::sanitized::SanitizedTransaction::try_new_from_fields(
        sanitized,
        Hash::new_unique(),
        false,
        signatures,
    )
    .unwrap();

    // Round-trip encode/decode
    let mut buf = Vec::new();
    tx.encode(&mut buf).unwrap();
    let decoded = tx3::sanitized::SanitizedTransaction::decode(&mut Cursor::new(&buf)).unwrap();
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
    let sanitized = msg3::SanitizedMessage::Legacy(legacy_message);
    let tx = tx3::sanitized::SanitizedTransaction::try_new_from_fields(
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
    let tx1 =
        tx3::sanitized::SanitizedTransaction::decode_ext(&mut Cursor::new(&buf1), Some(&mut dec))
            .unwrap();
    let tx2 =
        tx3::sanitized::SanitizedTransaction::decode_ext(&mut Cursor::new(&buf2), Some(&mut dec))
            .unwrap();
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
    let sanitized = msg3::SanitizedMessage::V0(loaded);
    let tx = tx3::sanitized::SanitizedTransaction::try_new_from_fields(
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
    let tx1 =
        tx3::sanitized::SanitizedTransaction::decode_ext(&mut Cursor::new(&buf1), Some(&mut dec))
            .unwrap();
    let tx2 =
        tx3::sanitized::SanitizedTransaction::decode_ext(&mut Cursor::new(&buf2), Some(&mut dec))
            .unwrap();
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
