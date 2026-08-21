//! In-memory codec benchmark for real mainnet message bodies dumped by
//! jetstreamer's `shred_compression --dump-msgs PATH N` mode.
//!
//! Usage:
//!   cargo run --release --features solana --example shred_wire_speed -- <dump.bin>

#![cfg_attr(not(all(feature = "solana", feature = "std")), allow(dead_code))]

#[cfg(all(feature = "solana", feature = "std"))]
mod real {
    use lencode::context::{DecoderContext, EncoderContext};
    use lencode::dedupe::{
        DedupeDecodeable, DedupeDecoder, DedupeEncodeable, DedupeEncoder, DefaultDedupeHasher,
    };
    use lencode::prelude::*;
    use serde::{Deserialize, Serialize};
    use std::hint::black_box;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use wincode::io::Cursor as WincodeCursor;
    use wincode::{SchemaRead, SchemaWrite};

    #[derive(Deserialize)]
    struct DumpIx {
        program_id_index: u8,
        accounts: Vec<u8>,
        data: Vec<u8>,
    }

    #[derive(Deserialize)]
    struct DumpMsg {
        account_keys: Vec<[u8; 32]>,
        recent_blockhash: [u8; 32],
        instructions: Vec<DumpIx>,
    }

    #[derive(Clone, PartialEq, Eq, Hash, Pack, Serialize, Deserialize, SchemaWrite, SchemaRead)]
    #[repr(transparent)]
    struct BenchPubkey([u8; 32]);

    impl DedupeEncodeable for BenchPubkey {
        type Hasher = DefaultDedupeHasher;
    }

    impl DedupeDecodeable for BenchPubkey {
        type Hasher = DefaultDedupeHasher;
    }

    #[derive(Clone, PartialEq, Serialize, Deserialize, SchemaWrite, SchemaRead, Encode, Decode)]
    struct BenchCompiledInstruction {
        program_id_index: u8,
        #[serde(with = "solana_short_vec")]
        #[wincode(with = "wincode::containers::Vec<_, wincode::len::ShortU16Len>")]
        accounts: Vec<u8>,
        #[serde(with = "solana_short_vec")]
        #[wincode(with = "wincode::containers::Vec<_, wincode::len::ShortU16Len>")]
        data: Vec<u8>,
    }

    #[derive(Clone, PartialEq, Serialize, Deserialize, SchemaWrite, SchemaRead, Encode, Decode)]
    struct BenchMessage {
        #[serde(with = "solana_short_vec")]
        #[wincode(with = "wincode::containers::Vec<_, wincode::len::ShortU16Len>")]
        account_keys: Vec<BenchPubkey>,
        recent_blockhash: [u8; 32],
        #[serde(with = "solana_short_vec")]
        #[wincode(with = "wincode::containers::Vec<_, wincode::len::ShortU16Len>")]
        instructions: Vec<BenchCompiledInstruction>,
    }

    struct Encoded {
        messages: Vec<Vec<u8>>,
        bytes: u64,
        fingerprint: u64,
    }

    #[derive(Default)]
    struct Measurements {
        encode: Vec<Duration>,
        decode: Vec<Duration>,
    }

    fn fingerprint(messages: &[Vec<u8>]) -> u64 {
        // FNV-1a over message lengths and bytes. This is a regression
        // fingerprint, not a cryptographic digest.
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for message in messages {
            for byte in (message.len() as u64).to_le_bytes().iter().chain(message) {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        hash
    }

    fn finish_encoded(messages: Vec<Vec<u8>>) -> Encoded {
        let bytes = messages.iter().map(|message| message.len() as u64).sum();
        let fingerprint = fingerprint(&messages);
        Encoded {
            messages,
            bytes,
            fingerprint,
        }
    }

    fn encode_wincode(messages: &[BenchMessage]) -> Encoded {
        let mut encoded = Vec::with_capacity(messages.len());
        for message in messages {
            let mut cursor = WincodeCursor::new(Vec::with_capacity(512));
            wincode::serialize_into(&mut cursor, message).unwrap();
            encoded.push(cursor.into_inner());
        }
        finish_encoded(encoded)
    }

    fn encode_lencode(messages: &[BenchMessage]) -> Encoded {
        let mut encoded = Vec::with_capacity(messages.len());
        for message in messages {
            let mut writer = lencode::io::VecWriter::with_capacity(512);
            message.encode_ext(&mut writer, None).unwrap();
            encoded.push(writer.into_inner());
        }
        finish_encoded(encoded)
    }

    fn encode_lencode_dict(
        messages: &[BenchMessage],
        frozen: &Arc<lencode::dedupe::FrozenEncoderState>,
    ) -> Encoded {
        let mut ctx = EncoderContext {
            dedupe: Some(DedupeEncoder::with_frozen(Arc::clone(frozen))),
            diff: None,
        };
        let mut encoded = Vec::with_capacity(messages.len());
        for message in messages {
            ctx.dedupe.as_mut().unwrap().clear();
            let mut writer = lencode::io::VecWriter::with_capacity(512);
            message.encode_ext(&mut writer, Some(&mut ctx)).unwrap();
            encoded.push(writer.into_inner());
        }
        finish_encoded(encoded)
    }

    fn decode_wincode(encoded: &Encoded) {
        for bytes in &encoded.messages {
            let message: BenchMessage = wincode::deserialize(bytes).unwrap();
            black_box(message);
        }
    }

    fn decode_lencode(encoded: &Encoded) {
        for bytes in &encoded.messages {
            let mut cursor = lencode::io::Cursor::new(bytes.as_slice());
            let message = BenchMessage::decode_ext(&mut cursor, None).unwrap();
            black_box(message);
        }
    }

    fn decode_lencode_dict(encoded: &Encoded, frozen: &Arc<lencode::dedupe::FrozenDecoderState>) {
        let mut ctx = DecoderContext {
            dedupe: Some(DedupeDecoder::with_frozen(Arc::clone(frozen))),
            diff: None,
        };
        for bytes in &encoded.messages {
            ctx.dedupe.as_mut().unwrap().clear();
            let mut cursor = lencode::io::Cursor::new(bytes.as_slice());
            let message = BenchMessage::decode_ext(&mut cursor, Some(&mut ctx)).unwrap();
            black_box(message);
        }
    }

    fn timed<T>(f: impl FnOnce() -> T) -> (Duration, T) {
        let start = Instant::now();
        let value = f();
        (start.elapsed(), value)
    }

    fn median(samples: &[Duration]) -> Duration {
        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        sorted[sorted.len() / 2]
    }

    fn report(
        name: &str,
        measurements: &Measurements,
        encoded: &Encoded,
        count: usize,
        wincode: Option<&Measurements>,
    ) {
        let enc = median(&measurements.encode);
        let dec = median(&measurements.decode);
        let enc_rate = count as f64 / enc.as_secs_f64();
        let dec_rate = count as f64 / dec.as_secs_f64();
        let (enc_ratio, dec_ratio) = wincode.map_or((1.0, 1.0), |base| {
            (
                median(&base.encode).as_secs_f64() / enc.as_secs_f64(),
                median(&base.decode).as_secs_f64() / dec.as_secs_f64(),
            )
        });
        println!(
            "RESULT name={name} enc_ns={} dec_ns={} bytes={} fingerprint={:016x}",
            enc.as_nanos(),
            dec.as_nanos(),
            encoded.bytes,
            encoded.fingerprint,
        );
        println!(
            "  {name:<13} enc {enc_rate:>10.0} msg/s  dec {dec_rate:>10.0} msg/s  \
             {:>6.1} B/msg  enc {enc_ratio:>5.2}x, dec {dec_ratio:>5.2}x vs wincode",
            encoded.bytes as f64 / count as f64,
        );
    }

    pub fn run() {
        let path = std::env::args().nth(1).unwrap_or_else(|| {
            eprintln!("usage: shred_wire_speed <dump.bin>");
            std::process::exit(2);
        });
        let rounds = std::env::var("WIRE_SPEED_RUNS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(7usize);
        assert!(rounds > 0);

        let raw = std::fs::read(&path).expect("read dump");
        let dump: Vec<DumpMsg> = bincode::serde::decode_from_slice(&raw, bincode::config::legacy())
            .expect("parse dump")
            .0;
        let messages: Vec<BenchMessage> = dump
            .into_iter()
            .map(|message| BenchMessage {
                account_keys: message.account_keys.into_iter().map(BenchPubkey).collect(),
                recent_blockhash: message.recent_blockhash,
                instructions: message
                    .instructions
                    .into_iter()
                    .map(|instruction| BenchCompiledInstruction {
                        program_id_index: instruction.program_id_index,
                        accounts: instruction.accounts,
                        data: instruction.data,
                    })
                    .collect(),
            })
            .collect();

        let prime_raw = std::fs::read(format!("{path}.prime")).expect("read prime table");
        assert_eq!(prime_raw.len() % 32, 0);
        let mut encoder_primer = DedupeEncoder::new();
        let mut decoder_primer = DedupeDecoder::new();
        for bytes in prime_raw.chunks_exact(32) {
            let pubkey = BenchPubkey(<[u8; 32]>::try_from(bytes).unwrap());
            encoder_primer.prime::<BenchPubkey, <BenchPubkey as DedupeEncodeable>::Hasher>(&pubkey);
            decoder_primer.prime::<BenchPubkey>(pubkey);
        }
        let frozen_encoder = Arc::new(encoder_primer.freeze());
        let frozen_decoder = Arc::new(decoder_primer.freeze());

        let wincode_encoded = encode_wincode(&messages);
        let lencode_encoded = encode_lencode(&messages);
        let dict_encoded = encode_lencode_dict(&messages, &frozen_encoder);

        if let Ok(profile) = std::env::var("WIRE_SPEED_PROFILE") {
            let iterations = std::env::var("WIRE_SPEED_PROFILE_ITERS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(50usize);
            for _ in 0..iterations {
                match profile.as_str() {
                    "wincode-encode" => {
                        black_box(encode_wincode(&messages));
                    }
                    "wincode-decode" => decode_wincode(black_box(&wincode_encoded)),
                    "lencode-encode" => {
                        black_box(encode_lencode(&messages));
                    }
                    "lencode-decode" => decode_lencode(black_box(&lencode_encoded)),
                    "dict-encode" => {
                        black_box(encode_lencode_dict(&messages, &frozen_encoder));
                    }
                    "dict-decode" => decode_lencode_dict(black_box(&dict_encoded), &frozen_decoder),
                    _ => panic!("unknown WIRE_SPEED_PROFILE mode: {profile}"),
                }
            }
            return;
        }

        for (message, bytes) in messages.iter().zip(&wincode_encoded.messages).take(1_000) {
            assert_eq!(
                &bincode::serde::encode_to_vec(message, bincode::config::legacy()).unwrap(),
                bytes,
                "wincode/bincode divergence",
            );
        }
        let mut verify_ctx = DecoderContext {
            dedupe: Some(DedupeDecoder::with_frozen(Arc::clone(&frozen_decoder))),
            diff: None,
        };
        for (expected, bytes) in messages.iter().zip(&dict_encoded.messages).take(10_000) {
            verify_ctx.dedupe.as_mut().unwrap().clear();
            let mut cursor = lencode::io::Cursor::new(bytes.as_slice());
            let decoded = BenchMessage::decode_ext(&mut cursor, Some(&mut verify_ctx)).unwrap();
            assert!(decoded == *expected, "dictionary round-trip mismatch");
        }

        // Untimed warm-up of every path.
        black_box(encode_wincode(&messages));
        black_box(encode_lencode(&messages));
        black_box(encode_lencode_dict(&messages, &frozen_encoder));
        decode_wincode(&wincode_encoded);
        decode_lencode(&lencode_encoded);
        decode_lencode_dict(&dict_encoded, &frozen_decoder);

        let mut measurements = [
            Measurements::default(),
            Measurements::default(),
            Measurements::default(),
        ];
        for round in 0..rounds {
            // Rotate order each round to spread out thermal/frequency drift.
            for index in [0, 1, 2].map(|index| (index + round) % 3) {
                let (duration, result) = match index {
                    0 => timed(|| encode_wincode(&messages)),
                    1 => timed(|| encode_lencode(&messages)),
                    2 => timed(|| encode_lencode_dict(&messages, &frozen_encoder)),
                    _ => unreachable!(),
                };
                let expected = [&wincode_encoded, &lencode_encoded, &dict_encoded][index];
                assert_eq!(result.bytes, expected.bytes);
                assert_eq!(result.fingerprint, expected.fingerprint);
                measurements[index].encode.push(duration);
            }
            for index in [0, 1, 2].map(|index| (index + round) % 3) {
                let duration = match index {
                    0 => timed(|| decode_wincode(&wincode_encoded)).0,
                    1 => timed(|| decode_lencode(&lencode_encoded)).0,
                    2 => timed(|| decode_lencode_dict(&dict_encoded, &frozen_decoder)).0,
                    _ => unreachable!(),
                };
                measurements[index].decode.push(duration);
            }
        }

        println!(
            "\n=== {path} | {} real mainnet message bodies | {rounds} trials ===",
            messages.len(),
        );
        report(
            "wincode",
            &measurements[0],
            &wincode_encoded,
            messages.len(),
            None,
        );
        report(
            "lencode",
            &measurements[1],
            &lencode_encoded,
            messages.len(),
            Some(&measurements[0]),
        );
        report(
            "lencode_dict",
            &measurements[2],
            &dict_encoded,
            messages.len(),
            Some(&measurements[0]),
        );
        println!(
            "compression: lencode_dict is {:.2}% smaller than wincode",
            100.0 * (1.0 - dict_encoded.bytes as f64 / wincode_encoded.bytes as f64),
        );
        println!("wire sanity and sampled dictionary round trips verified");
    }
}

#[cfg(all(feature = "solana", feature = "std"))]
fn main() {
    real::run();
}

#[cfg(not(all(feature = "solana", feature = "std")))]
fn main() {
    eprintln!("build with --features solana (std default) to run shred_wire_speed");
}
