use solana_sdk::pubkey::Pubkey;

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
