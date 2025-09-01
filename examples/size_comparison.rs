use lencode::{
    dedupe::{DedupeDecoder, DedupeEncoder},
    prelude::*,
};
use rand::Rng;
use solana_sdk::pubkey::Pubkey;
use std::io::Cursor;

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
    let mut encoder = DedupeEncoder::new();
    let mut cursor = Cursor::new(Vec::new());
    all_pubkeys.encode(&mut cursor, Some(&mut encoder)).unwrap();
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
        encoder.len(),
        all_pubkeys.len()
    );

    // Verify we can decode correctly
    let mut decoder = DedupeDecoder::new();
    let mut cursor = Cursor::new(&lencode_data);
    let decoded: Vec<Pubkey> = Vec::decode(&mut cursor, Some(&mut decoder)).unwrap();

    assert_eq!(all_pubkeys.len(), decoded.len());
    assert_eq!(all_pubkeys, decoded);
    println!("✓ Decoding verification passed");
}
