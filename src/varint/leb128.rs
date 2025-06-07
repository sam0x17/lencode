use crate::prelude::*;

pub enum Leb128Capped {}

impl Scheme for Leb128Capped {
    fn encode<I: Integer>(val: I, writer: impl Write) -> Result<usize> {
        let mut bytes_written = 0;

        todo!()
    }

    fn decode<I: Integer>(_reader: impl Read) -> Result<I> {
        todo!()
    }
}
