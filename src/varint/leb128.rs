use crate::prelude::*;

/// Capped LEB128 encoding scheme.
///
/// Values are encoded in two possible ways:
///
/// 1. **Raw big-endian**  
///    If the integer’s most significant bit (the very first output bit) is 1, then the remaining
///    bits are simply the full integer in MSB-first, big-endian order.
///
/// 2. **LEB128**  
///    Otherwise (first bit = 0), we use a modified LEB128 where each output byte’s top bit is
///    the *terminator* flag (0 = more bytes follow, 1 = this is the last byte), and the lower
///    seven bits carry the payload.
///
/// This "cap" ensures that small values pay only the LEB128 overhead, but once you exceed the
/// native type’s size you fall back to a fixed-width big-endian representation.

pub enum Leb128Capped {}

impl Scheme for Leb128Capped {
    fn encode<I: Integer>(writer: impl Write) -> Result<usize> {
        todo!()
    }

    fn decode<I: Integer>(reader: impl Read) -> Result<I> {
        todo!()
    }
}
