#[cfg(feature = "solana")]
use lencode::{
    dedupe::{DedupeDecoder, DedupeEncoder},
    prelude::*,
};
#[cfg(feature = "solana")]
use rand::Rng;
#[cfg(feature = "solana")]
use solana_sdk::{
    hash::Hash,
    instruction::CompiledInstruction,
    message::{Message, MessageHeader, VersionedMessage, v0},
    pubkey::Pubkey,
    signature::Signature,
    transaction::VersionedTransaction,
};
#[cfg(feature = "solana")]
use std::{io::Cursor, time::Instant};

#[cfg(feature = "solana")]
fn gen_pubkeys(count: usize, dup_ratio: f64) -> Vec<Pubkey> {
    let mut rng = rand::rng();
    let uniq_count = ((count as f64) * (1.0 - dup_ratio)).round() as usize;
    let dup_count = count.saturating_sub(uniq_count);
    let uniques: Vec<Pubkey> = (0..uniq_count).map(|_| Pubkey::new_unique()).collect();
    let mut all = uniques.clone();
    for _ in 0..dup_count {
        let idx = rng.random_range(0..uniques.len());
        all.push(uniques[idx]);
    }
    all
}

#[cfg(feature = "solana")]
fn build_legacy_tx(dup_ratio: f64) -> VersionedTransaction {
    let mut rng = rand::rng();
    let account_keys = gen_pubkeys(16, dup_ratio);
    let header = MessageHeader {
        num_required_signatures: 2,
        num_readonly_signed_accounts: 0,
        num_readonly_unsigned_accounts: 2,
    };
    let instructions = (0..3)
        .map(|_| CompiledInstruction {
            program_id_index: rng.random_range(0..account_keys.len()) as u8,
            accounts: (0..3)
                .map(|_| rng.random_range(0..account_keys.len()) as u8)
                .collect(),
            data: (0..rng.random_range(16..48))
                .map(|_| rng.random())
                .collect(),
        })
        .collect();
    let legacy_message = Message {
        header,
        account_keys,
        recent_blockhash: Hash::new_unique(),
        instructions,
    };
    VersionedTransaction {
        signatures: vec![Signature::default(), Signature::default()],
        message: VersionedMessage::Legacy(legacy_message),
    }
}

#[cfg(feature = "solana")]
fn build_v0_tx(dup_ratio: f64) -> VersionedTransaction {
    let mut rng = rand::rng();
    let account_keys = gen_pubkeys(12, dup_ratio);
    let header = MessageHeader {
        num_required_signatures: 1,
        num_readonly_signed_accounts: 0,
        num_readonly_unsigned_accounts: 2,
    };
    let instructions = (0..3)
        .map(|_| CompiledInstruction {
            program_id_index: rng.random_range(0..account_keys.len()) as u8,
            accounts: (0..3)
                .map(|_| rng.random_range(0..account_keys.len()) as u8)
                .collect(),
            data: (0..rng.random_range(16..48))
                .map(|_| rng.random())
                .collect(),
        })
        .collect();
    let v0_message = v0::Message {
        header,
        account_keys,
        recent_blockhash: Hash::new_unique(),
        instructions,
        address_table_lookups: vec![],
    };
    VersionedTransaction {
        signatures: vec![Signature::default()],
        message: VersionedMessage::V0(v0_message),
    }
}

#[cfg(feature = "solana")]
fn main() {
    const TX_COUNT: usize = 4000;
    const DUP_RATIO: f64 = 0.80; // 80% duplicates

    // Build a mixed set of versioned transactions (half legacy, half v0)
    let mut vtxs: Vec<VersionedTransaction> = Vec::with_capacity(TX_COUNT);
    for i in 0..TX_COUNT {
        if i % 2 == 0 {
            vtxs.push(build_legacy_tx(DUP_RATIO));
        } else {
            vtxs.push(build_v0_tx(DUP_RATIO));
        }
    }

    // bincode: serialize VersionedTransaction via serde
    let t0 = Instant::now();
    let bincode_bytes = bincode::serde::encode_to_vec(&vtxs, bincode::config::standard()).unwrap();
    let t_bincode = t0.elapsed();

    // lencode: enable dedupe across the entire set
    let mut lencode_buf = Vec::new();
    let mut enc = DedupeEncoder::with_capacity(4096, 8);
    let t1 = Instant::now();
    vtxs.encode_ext(&mut lencode_buf, Some(&mut enc)).unwrap();
    let t_lencode = t1.elapsed();

    let bincode_len = bincode_bytes.len();
    let lencode_len = lencode_buf.len();
    let ratio = lencode_len as f64 / bincode_len as f64;
    let savings = 100.0 * (1.0 - ratio);

    println!(
        "Transactions: {} (dup pubkeys ~{:.0}% per set)",
        TX_COUNT,
        DUP_RATIO * 100.0
    );
    println!("bincode size: {} bytes", bincode_len);
    println!("lencode size: {} bytes (dedupe on)", lencode_len);
    println!("compression ratio (lencode/bincode): {:.3}", ratio);
    println!("space savings vs bincode: {:.1}%", savings);
    println!("unique values captured by dedupe: {}", enc.len());
    println!("bincode encode time: {:?}", t_bincode);
    println!("lencode encode time: {:?}", t_lencode);

    // Verify we can decode the lencode stream
    let mut dec = DedupeDecoder::with_capacity(4096);
    let decoded: Vec<VersionedTransaction> =
        Vec::decode_ext(&mut Cursor::new(&lencode_buf), Some(&mut dec)).unwrap();
    assert_eq!(decoded, vtxs);
    println!("✓ Round-trip decode verified");
}

#[cfg(not(feature = "solana"))]
fn main() {
    println!("This example requires the 'solana' feature to be enabled.");
    println!("Run with: cargo run --example versioned_tx_compression --features=solana");
}
