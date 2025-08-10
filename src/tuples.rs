use crate::prelude::*;

impl<T: Encode> Encode for (T,) {
    fn encode(&self, writer: &mut impl Write) -> Result<usize> {
        self.0.encode(writer)
    }
}

impl<T: Decode> Decode for (T,) {
    #[inline(always)]
    fn decode(reader: &mut impl Read) -> Result<Self> {
        Ok((T::decode(reader)?,))
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<A: Encode, B: Encode> Encode for (A, B) {
    #[inline(always)]
    fn encode(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += self.0.encode(writer)?;
        total_written += self.1.encode(writer)?;
        Ok(total_written)
    }
}

impl<A: Decode, B: Decode> Decode for (A, B) {
    #[inline(always)]
    fn decode(reader: &mut impl Read) -> Result<Self> {
        Ok((A::decode(reader)?, B::decode(reader)?))
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<A: Encode, B: Encode, C: Encode> Encode for (A, B, C) {
    #[inline(always)]
    fn encode(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += self.0.encode(writer)?;
        total_written += self.1.encode(writer)?;
        total_written += self.2.encode(writer)?;
        Ok(total_written)
    }
}

impl<A: Decode, B: Decode, C: Decode> Decode for (A, B, C) {
    #[inline(always)]
    fn decode(reader: &mut impl Read) -> Result<Self> {
        Ok((A::decode(reader)?, B::decode(reader)?, C::decode(reader)?))
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<A: Encode, B: Encode, C: Encode, D: Encode> Encode for (A, B, C, D) {
    #[inline(always)]
    fn encode(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += self.0.encode(writer)?;
        total_written += self.1.encode(writer)?;
        total_written += self.2.encode(writer)?;
        total_written += self.3.encode(writer)?;
        Ok(total_written)
    }
}

impl<A: Decode, B: Decode, C: Decode, D: Decode> Decode for (A, B, C, D) {
    #[inline(always)]
    fn decode(reader: &mut impl Read) -> Result<Self> {
        Ok((
            A::decode(reader)?,
            B::decode(reader)?,
            C::decode(reader)?,
            D::decode(reader)?,
        ))
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<A: Encode, B: Encode, C: Encode, D: Encode, E: Encode> Encode for (A, B, C, D, E) {
    #[inline(always)]
    fn encode(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += self.0.encode(writer)?;
        total_written += self.1.encode(writer)?;
        total_written += self.2.encode(writer)?;
        total_written += self.3.encode(writer)?;
        total_written += self.4.encode(writer)?;
        Ok(total_written)
    }
}

impl<A: Decode, B: Decode, C: Decode, D: Decode, E: Decode> Decode for (A, B, C, D, E) {
    #[inline(always)]
    fn decode(reader: &mut impl Read) -> Result<Self> {
        Ok((
            A::decode(reader)?,
            B::decode(reader)?,
            C::decode(reader)?,
            D::decode(reader)?,
            E::decode(reader)?,
        ))
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<A: Encode, B: Encode, C: Encode, D: Encode, E: Encode, F: Encode> Encode
    for (A, B, C, D, E, F)
{
    #[inline(always)]
    fn encode(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += self.0.encode(writer)?;
        total_written += self.1.encode(writer)?;
        total_written += self.2.encode(writer)?;
        total_written += self.3.encode(writer)?;
        total_written += self.4.encode(writer)?;
        total_written += self.5.encode(writer)?;
        Ok(total_written)
    }
}

impl<A: Decode, B: Decode, C: Decode, D: Decode, E: Decode, F: Decode> Decode
    for (A, B, C, D, E, F)
{
    #[inline(always)]
    fn decode(reader: &mut impl Read) -> Result<Self> {
        Ok((
            A::decode(reader)?,
            B::decode(reader)?,
            C::decode(reader)?,
            D::decode(reader)?,
            E::decode(reader)?,
            F::decode(reader)?,
        ))
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<A: Encode, B: Encode, C: Encode, D: Encode, E: Encode, F: Encode, G: Encode> Encode
    for (A, B, C, D, E, F, G)
{
    #[inline(always)]
    fn encode(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += self.0.encode(writer)?;
        total_written += self.1.encode(writer)?;
        total_written += self.2.encode(writer)?;
        total_written += self.3.encode(writer)?;
        total_written += self.4.encode(writer)?;
        total_written += self.5.encode(writer)?;
        total_written += self.6.encode(writer)?;
        Ok(total_written)
    }
}

impl<A: Decode, B: Decode, C: Decode, D: Decode, E: Decode, F: Decode, G: Decode> Decode
    for (A, B, C, D, E, F, G)
{
    #[inline(always)]
    fn decode(reader: &mut impl Read) -> Result<Self> {
        Ok((
            A::decode(reader)?,
            B::decode(reader)?,
            C::decode(reader)?,
            D::decode(reader)?,
            E::decode(reader)?,
            F::decode(reader)?,
            G::decode(reader)?,
        ))
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<A: Encode, B: Encode, C: Encode, D: Encode, E: Encode, F: Encode, G: Encode, H: Encode> Encode
    for (A, B, C, D, E, F, G, H)
{
    #[inline(always)]
    fn encode(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += self.0.encode(writer)?;
        total_written += self.1.encode(writer)?;
        total_written += self.2.encode(writer)?;
        total_written += self.3.encode(writer)?;
        total_written += self.4.encode(writer)?;
        total_written += self.5.encode(writer)?;
        total_written += self.6.encode(writer)?;
        total_written += self.7.encode(writer)?;
        Ok(total_written)
    }
}

impl<A: Decode, B: Decode, C: Decode, D: Decode, E: Decode, F: Decode, G: Decode, H: Decode> Decode
    for (A, B, C, D, E, F, G, H)
{
    #[inline(always)]
    fn decode(reader: &mut impl Read) -> Result<Self> {
        Ok((
            A::decode(reader)?,
            B::decode(reader)?,
            C::decode(reader)?,
            D::decode(reader)?,
            E::decode(reader)?,
            F::decode(reader)?,
            G::decode(reader)?,
            H::decode(reader)?,
        ))
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<
    A: Encode,
    B: Encode,
    C: Encode,
    D: Encode,
    E: Encode,
    F: Encode,
    G: Encode,
    H: Encode,
    I: Encode,
> Encode for (A, B, C, D, E, F, G, H, I)
{
    #[inline(always)]
    fn encode(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += self.0.encode(writer)?;
        total_written += self.1.encode(writer)?;
        total_written += self.2.encode(writer)?;
        total_written += self.3.encode(writer)?;
        total_written += self.4.encode(writer)?;
        total_written += self.5.encode(writer)?;
        total_written += self.6.encode(writer)?;
        total_written += self.7.encode(writer)?;
        total_written += self.8.encode(writer)?;
        Ok(total_written)
    }
}

impl<
    A: Decode,
    B: Decode,
    C: Decode,
    D: Decode,
    E: Decode,
    F: Decode,
    G: Decode,
    H: Decode,
    I: Decode,
> Decode for (A, B, C, D, E, F, G, H, I)
{
    #[inline(always)]
    fn decode(reader: &mut impl Read) -> Result<Self> {
        Ok((
            A::decode(reader)?,
            B::decode(reader)?,
            C::decode(reader)?,
            D::decode(reader)?,
            E::decode(reader)?,
            F::decode(reader)?,
            G::decode(reader)?,
            H::decode(reader)?,
            I::decode(reader)?,
        ))
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<
    A: Encode,
    B: Encode,
    C: Encode,
    D: Encode,
    E: Encode,
    F: Encode,
    G: Encode,
    H: Encode,
    I: Encode,
    J: Encode,
> Encode for (A, B, C, D, E, F, G, H, I, J)
{
    #[inline(always)]
    fn encode(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += self.0.encode(writer)?;
        total_written += self.1.encode(writer)?;
        total_written += self.2.encode(writer)?;
        total_written += self.3.encode(writer)?;
        total_written += self.4.encode(writer)?;
        total_written += self.5.encode(writer)?;
        total_written += self.6.encode(writer)?;
        total_written += self.7.encode(writer)?;
        total_written += self.8.encode(writer)?;
        total_written += self.9.encode(writer)?;
        Ok(total_written)
    }
}

impl<
    A: Decode,
    B: Decode,
    C: Decode,
    D: Decode,
    E: Decode,
    F: Decode,
    G: Decode,
    H: Decode,
    I: Decode,
    J: Decode,
> Decode for (A, B, C, D, E, F, G, H, I, J)
{
    #[inline(always)]
    fn decode(reader: &mut impl Read) -> Result<Self> {
        Ok((
            A::decode(reader)?,
            B::decode(reader)?,
            C::decode(reader)?,
            D::decode(reader)?,
            E::decode(reader)?,
            F::decode(reader)?,
            G::decode(reader)?,
            H::decode(reader)?,
            I::decode(reader)?,
            J::decode(reader)?,
        ))
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<
    A: Encode,
    B: Encode,
    C: Encode,
    D: Encode,
    E: Encode,
    F: Encode,
    G: Encode,
    H: Encode,
    I: Encode,
    J: Encode,
    K: Encode,
> Encode for (A, B, C, D, E, F, G, H, I, J, K)
{
    #[inline(always)]
    fn encode(&self, writer: &mut impl Write) -> Result<usize> {
        let mut total_written = 0;
        total_written += self.0.encode(writer)?;
        total_written += self.1.encode(writer)?;
        total_written += self.2.encode(writer)?;
        total_written += self.3.encode(writer)?;
        total_written += self.4.encode(writer)?;
        total_written += self.5.encode(writer)?;
        total_written += self.6.encode(writer)?;
        total_written += self.7.encode(writer)?;
        total_written += self.8.encode(writer)?;
        total_written += self.9.encode(writer)?;
        total_written += self.10.encode(writer)?;
        Ok(total_written)
    }
}

impl<
    A: Decode,
    B: Decode,
    C: Decode,
    D: Decode,
    E: Decode,
    F: Decode,
    G: Decode,
    H: Decode,
    I: Decode,
    J: Decode,
    K: Decode,
> Decode for (A, B, C, D, E, F, G, H, I, J, K)
{
    #[inline(always)]
    fn decode(reader: &mut impl Read) -> Result<Self> {
        Ok((
            A::decode(reader)?,
            B::decode(reader)?,
            C::decode(reader)?,
            D::decode(reader)?,
            E::decode(reader)?,
            F::decode(reader)?,
            G::decode(reader)?,
            H::decode(reader)?,
            I::decode(reader)?,
            J::decode(reader)?,
            K::decode(reader)?,
        ))
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

#[test]
fn test_7_tuple_encode_decode() {
    let tuple = (1u8, 2u16, 3u32, 4u64, 5u128, 6usize, 7i8);
    let mut buffer = Vec::new();

    let written = tuple.encode(&mut buffer).unwrap();
    assert_eq!(written, 7);

    let decoded: (u8, u16, u32, u64, u128, usize, i8) =
        Decode::decode(&mut Cursor::new(&buffer[..])).unwrap();
    assert_eq!(decoded, tuple);
}
