use solana_sdk::pubkey::Pubkey;

use crate::prelude::*;

impl Encode for Pubkey {
    #[inline(always)]
    fn encode(&self, writer: &mut impl Write) -> Result<usize> {
        self.as_array().encode(writer)
    }
}

impl Decode for Pubkey {
    #[inline(always)]
    fn decode(reader: &mut impl Read) -> Result<Self> {
        Ok(Pubkey::new_from_array(<[u8; 32]>::decode(reader)?))
    }
}

#[test]
fn test_encode_decode_pubkey() {
    for i in 0..1000 {
        let pubkey = Pubkey::new_unique();
        let mut buf = [0u8; 32];
        let mut cursor = Cursor::new(&mut buf);
        let n = pubkey.encode(&mut cursor).unwrap();
        println!("Iteration {}: Encoded {} bytes", i, n);
        let decoded_pubkey = Pubkey::decode(&mut Cursor::new(&mut buf)).unwrap();
        assert_eq!(pubkey, decoded_pubkey, "Failed for iteration {}", i);
    }
}
