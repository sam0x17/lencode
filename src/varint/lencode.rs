use crate::prelude::*;

/// The Lencode integer encoding [`Scheme`] is designed to encode integers in a variable-length
/// format that is efficient for both small and large values both in terms of space and speed.
///
/// Lencode is a hybrid scheme where small integers <= 127 are encoded in a single byte (the
/// first bit is a flag indicating whether the integer is small or large, 0 means small and 1
/// means large). Large integers > 127 have the length of their raw bytes encoded in the
/// remaining 7 bits of the first byte, followed by the raw bytes of the integer. In this way
/// we never waste more than one byte for large integers, and small integers always fit within
/// a single byte. The only case where we waste more than the full byte size of an integer
/// primitive is when the value is large enough to require 1s in the most significant byte, in
/// which case we waste one additional byte for the length encoding.
///
/// Integers that need more than 127 bytes in their standard two's complement representation
/// are not supported by this scheme, but such integers are incredibly large and unlikely to be
/// used in practice.
pub enum Lencode {}

impl Scheme for Lencode {
    fn encode<I: Integer>(val: I, writer: impl Write) -> Result<usize> {
        let mut bytes_written = 0;

        todo!()
    }

    fn decode<I: Integer>(_reader: impl Read) -> Result<I> {
        todo!()
    }
}
