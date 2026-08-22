#[cfg(feature = "solana-types")]
use lencode::{
    context::{DecoderContext, EncoderContext},
    dedupe::{DedupeDecoder, DedupeEncoder},
    prelude::*,
};
#[cfg(feature = "solana-types")]
use rand::RngExt;
#[cfg(feature = "solana-types")]
use solana_hash::Hash;
#[cfg(feature = "solana-types")]
use solana_message::{
    Message, MessageHeader, VersionedMessage, compiled_instruction::CompiledInstruction, v0,
};
#[cfg(feature = "solana-types")]
use solana_pubkey::Pubkey;
#[cfg(feature = "solana-types")]
use solana_signature::Signature;
#[cfg(feature = "solana-types")]
use solana_transaction::versioned::VersionedTransaction;
#[cfg(feature = "solana-types")]
use std::{io::Cursor, time::Instant};

#[cfg(feature = "solana-types")]
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

#[cfg(feature = "solana-types")]
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

#[cfg(feature = "solana-types")]
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

#[cfg(feature = "solana-types")]
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

    // Wincode: serialize the current reference VersionedTransaction.
    let t0 = Instant::now();
    let wincode_bytes = wincode::serialize(&vtxs).unwrap();
    let t_wincode = t0.elapsed();

    // lencode: enable dedupe across the entire set
    let mut lencode_buf = Vec::new();
    let mut enc = EncoderContext {
        dedupe: Some(DedupeEncoder::with_capacity(4096, 8)),
        diff: None,
    };
    let t1 = Instant::now();
    vtxs.encode_ext(&mut lencode_buf, Some(&mut enc)).unwrap();
    let t_lencode = t1.elapsed();

    let wincode_len = wincode_bytes.len();
    let lencode_len = lencode_buf.len();
    let ratio = lencode_len as f64 / wincode_len as f64;
    let savings = 100.0 * (1.0 - ratio);

    println!(
        "Transactions: {} (dup pubkeys ~{:.0}% per set)",
        TX_COUNT,
        DUP_RATIO * 100.0
    );
    println!("wincode size: {} bytes", wincode_len);
    println!("lencode size: {} bytes (dedupe on)", lencode_len);
    println!("compression ratio (lencode/wincode): {:.3}", ratio);
    println!("space savings vs wincode: {:.1}%", savings);
    println!(
        "unique values captured by dedupe: {}",
        enc.dedupe.as_ref().unwrap().len()
    );
    println!("wincode encode time: {:?}", t_wincode);
    println!("lencode encode time: {:?}", t_lencode);

    // Verify we can decode the lencode stream
    let mut dec = DecoderContext {
        dedupe: Some(DedupeDecoder::with_capacity(4096)),
        diff: None,
    };
    let decoded: Vec<VersionedTransaction> =
        Vec::decode_ext(&mut Cursor::new(&lencode_buf), Some(&mut dec)).unwrap();
    assert_eq!(decoded, vtxs);
    println!("✓ Round-trip decode verified");
}

#[cfg(not(feature = "solana-types"))]
fn main() {
    println!("This example requires the 'solana-types' feature to be enabled.");
    println!("Run with: cargo run --example versioned_tx_compression --features=solana-types");
}
