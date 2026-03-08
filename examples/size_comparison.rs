#[cfg(feature = "solana")]
use lencode::{
    context::{DecoderContext, EncoderContext},
    dedupe::{DedupeDecoder, DedupeEncoder},
    prelude::*,
};
#[cfg(feature = "solana")]
use rand::RngExt;
#[cfg(feature = "solana")]
use std::io::Cursor;

#[cfg(feature = "solana")]
use solana_pubkey::Pubkey;

#[cfg(feature = "solana")]
fn main() {
    // Create a vector of 1000 pubkeys where 50% are duplicates
    let mut rng = rand::rng();
    let unique_pubkeys: Vec<Pubkey> = (0..500).map(|_| Pubkey::new_unique()).collect();

    // Create duplicates by randomly selecting from unique pubkeys
    let duplicates: Vec<Pubkey> = (0..500)
        .map(|_| {
            let idx = rng.random_range(0..unique_pubkeys.len());
            unique_pubkeys[idx]
        })
        .collect();

    // Combine vectors
    let mut all_pubkeys = unique_pubkeys;
    all_pubkeys.extend(duplicates);

    // Encode with borsh
    let borsh_data = borsh::to_vec(&all_pubkeys).unwrap();

    // Encode with lencode + deduplication
    let mut ctx = EncoderContext {
        dedupe: Some(DedupeEncoder::with_capacity(1000, 1)),
        diff: None,
    };
    let mut cursor = Cursor::new(Vec::new());
    all_pubkeys.encode_ext(&mut cursor, Some(&mut ctx)).unwrap();
    let lencode_data = cursor.into_inner();

    println!("Vector size: {} pubkeys", all_pubkeys.len());
    println!("Borsh encoded size: {} bytes", borsh_data.len());
    println!("Lencode encoded size: {} bytes", lencode_data.len());
    println!(
        "Space savings: {:.1}%",
        100.0 * (1.0 - lencode_data.len() as f64 / borsh_data.len() as f64)
    );
    println!(
        "Unique values stored: {} out of {} total",
        ctx.dedupe.as_ref().unwrap().len(),
        all_pubkeys.len()
    );

    // Verify we can decode correctly
    let mut dec_ctx = DecoderContext {
        dedupe: Some(DedupeDecoder::with_capacity(1000)),
        diff: None,
    };
    let mut cursor = Cursor::new(&lencode_data);
    let decoded: Vec<Pubkey> = Vec::decode_ext(&mut cursor, Some(&mut dec_ctx)).unwrap();

    assert_eq!(all_pubkeys.len(), decoded.len());
    assert_eq!(all_pubkeys, decoded);
    println!("✓ Decoding verification passed");
}

#[cfg(not(feature = "solana"))]
fn main() {
    println!("This example requires the 'solana' feature to be enabled.");
    println!("Run with: cargo run --example size_comparison --features=solana");
}
