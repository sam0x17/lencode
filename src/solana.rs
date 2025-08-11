use solana_sdk::{
    hash::{HASH_BYTES, Hash},
    pubkey::Pubkey,
    signature::{SIGNATURE_BYTES, Signature},
    transaction::SanitizedTransaction,
};

use crate::prelude::*;

// note: Pubkeys are completely uncompressible using varint encoding so it is better to encode
// them as raw bytes to save the extra one byte of overhead that varint encoding would add.
impl Encode for Pubkey {
    #[inline(always)]
    fn encode(&self, writer: &mut impl Write) -> Result<usize> {
        self.as_array().encode(writer)
    }
}

impl Decode for Pubkey {
    #[inline(always)]
    fn decode(reader: &mut impl Read) -> Result<Self> {
        Ok(Pubkey::new_from_array(decode(reader)?))
    }
}

impl Encode for Hash {
    fn encode(&self, writer: &mut impl Write) -> Result<usize> {
        self.to_bytes().encode(writer)
    }
}

impl Decode for Hash {
    fn decode(reader: &mut impl Read) -> Result<Self> {
        let bytes = <[u8; HASH_BYTES]>::decode(reader)?;
        Ok(Hash::new_from_array(bytes))
    }
}

impl Encode for Signature {
    fn encode(&self, writer: &mut impl Write) -> Result<usize> {
        self.as_array().encode(writer)
    }
}

impl Decode for Signature {
    fn decode(reader: &mut impl Read) -> Result<Self> {
        let sig: [u8; SIGNATURE_BYTES] = decode(reader)?;
        Ok(Signature::from(sig))
    }
}

impl Encode for SanitizedTransaction {
    #[inline(always)]
    fn encode(&self, writer: &mut impl Write) -> Result<usize> {
        todo!()
    }
}

#[test]
fn test_encode_decode_pubkey() {
    for _ in 0..1000 {
        let pubkey = Pubkey::new_unique();
        let mut buf = [0u8; 64];
        let mut cursor = Cursor::new(&mut buf);
        let n = pubkey.encode(&mut cursor).unwrap();
        assert_eq!(n, 32);
        let decoded_pubkey = Pubkey::decode(&mut Cursor::new(&mut buf)).unwrap();
        assert_eq!(pubkey, decoded_pubkey);
    }
}
