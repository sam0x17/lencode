use crate::prelude::*;

impl<T: Encode> Encode for (T,) {
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        self.0.encode_ext(writer, dedupe_encoder)
    }
}

impl<T: Decode> Decode for (T,) {
    #[inline(always)]
    fn decode(
        reader: &mut impl Read,
        dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        Ok((T::decode(reader, dedupe_decoder)?,))
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<A: Encode, B: Encode> Encode for (A, B) {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += self.0.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.1.encode_ext(writer, dedupe_encoder)?;
        Ok(total_written)
    }
}

impl<A: Decode, B: Decode> Decode for (A, B) {
    #[inline(always)]
    fn decode(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        Ok((
            A::decode(reader, dedupe_decoder.as_deref_mut())?,
            B::decode(reader, dedupe_decoder)?,
        ))
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<A: Encode, B: Encode, C: Encode> Encode for (A, B, C) {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += self.0.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.1.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.2.encode_ext(writer, dedupe_encoder)?;
        Ok(total_written)
    }
}

impl<A: Decode, B: Decode, C: Decode> Decode for (A, B, C) {
    #[inline(always)]
    fn decode(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        Ok((
            A::decode(reader, dedupe_decoder.as_deref_mut())?,
            B::decode(reader, dedupe_decoder.as_deref_mut())?,
            C::decode(reader, dedupe_decoder)?,
        ))
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<A: Encode, B: Encode, C: Encode, D: Encode> Encode for (A, B, C, D) {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += self.0.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.1.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.2.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.3.encode_ext(writer, dedupe_encoder)?;
        Ok(total_written)
    }
}

impl<A: Decode, B: Decode, C: Decode, D: Decode> Decode for (A, B, C, D) {
    #[inline(always)]
    fn decode(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        Ok((
            A::decode(reader, dedupe_decoder.as_deref_mut())?,
            B::decode(reader, dedupe_decoder.as_deref_mut())?,
            C::decode(reader, dedupe_decoder.as_deref_mut())?,
            D::decode(reader, dedupe_decoder)?,
        ))
    }

    fn decode_len(_reader: &mut impl Read) -> Result<usize> {
        unimplemented!()
    }
}

impl<A: Encode, B: Encode, C: Encode, D: Encode, E: Encode> Encode for (A, B, C, D, E) {
    #[inline(always)]
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += self.0.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.1.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.2.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.3.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.4.encode_ext(writer, dedupe_encoder)?;
        Ok(total_written)
    }
}

impl<A: Decode, B: Decode, C: Decode, D: Decode, E: Decode> Decode for (A, B, C, D, E) {
    #[inline(always)]
    fn decode(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        Ok((
            A::decode(reader, dedupe_decoder.as_deref_mut())?,
            B::decode(reader, dedupe_decoder.as_deref_mut())?,
            C::decode(reader, dedupe_decoder.as_deref_mut())?,
            D::decode(reader, dedupe_decoder.as_deref_mut())?,
            E::decode(reader, dedupe_decoder)?,
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
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += self.0.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.1.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.2.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.3.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.4.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.5.encode_ext(writer, dedupe_encoder)?;
        Ok(total_written)
    }
}

impl<A: Decode, B: Decode, C: Decode, D: Decode, E: Decode, F: Decode> Decode
    for (A, B, C, D, E, F)
{
    #[inline(always)]
    fn decode(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        Ok((
            A::decode(reader, dedupe_decoder.as_deref_mut())?,
            B::decode(reader, dedupe_decoder.as_deref_mut())?,
            C::decode(reader, dedupe_decoder.as_deref_mut())?,
            D::decode(reader, dedupe_decoder.as_deref_mut())?,
            E::decode(reader, dedupe_decoder.as_deref_mut())?,
            F::decode(reader, dedupe_decoder)?,
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
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += self.0.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.1.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.2.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.3.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.4.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.5.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.6.encode_ext(writer, dedupe_encoder)?;
        Ok(total_written)
    }
}

impl<A: Decode, B: Decode, C: Decode, D: Decode, E: Decode, F: Decode, G: Decode> Decode
    for (A, B, C, D, E, F, G)
{
    #[inline(always)]
    fn decode(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        Ok((
            A::decode(reader, dedupe_decoder.as_deref_mut())?,
            B::decode(reader, dedupe_decoder.as_deref_mut())?,
            C::decode(reader, dedupe_decoder.as_deref_mut())?,
            D::decode(reader, dedupe_decoder.as_deref_mut())?,
            E::decode(reader, dedupe_decoder.as_deref_mut())?,
            F::decode(reader, dedupe_decoder.as_deref_mut())?,
            G::decode(reader, dedupe_decoder)?,
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
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += self.0.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.1.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.2.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.3.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.4.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.5.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.6.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.7.encode_ext(writer, dedupe_encoder)?;
        Ok(total_written)
    }
}

impl<A: Decode, B: Decode, C: Decode, D: Decode, E: Decode, F: Decode, G: Decode, H: Decode> Decode
    for (A, B, C, D, E, F, G, H)
{
    #[inline(always)]
    fn decode(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        Ok((
            A::decode(reader, dedupe_decoder.as_deref_mut())?,
            B::decode(reader, dedupe_decoder.as_deref_mut())?,
            C::decode(reader, dedupe_decoder.as_deref_mut())?,
            D::decode(reader, dedupe_decoder.as_deref_mut())?,
            E::decode(reader, dedupe_decoder.as_deref_mut())?,
            F::decode(reader, dedupe_decoder.as_deref_mut())?,
            G::decode(reader, dedupe_decoder.as_deref_mut())?,
            H::decode(reader, dedupe_decoder)?,
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
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += self.0.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.1.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.2.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.3.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.4.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.5.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.6.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.7.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.8.encode_ext(writer, dedupe_encoder)?;
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
    fn decode(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        Ok((
            A::decode(reader, dedupe_decoder.as_deref_mut())?,
            B::decode(reader, dedupe_decoder.as_deref_mut())?,
            C::decode(reader, dedupe_decoder.as_deref_mut())?,
            D::decode(reader, dedupe_decoder.as_deref_mut())?,
            E::decode(reader, dedupe_decoder.as_deref_mut())?,
            F::decode(reader, dedupe_decoder.as_deref_mut())?,
            G::decode(reader, dedupe_decoder.as_deref_mut())?,
            H::decode(reader, dedupe_decoder.as_deref_mut())?,
            I::decode(reader, dedupe_decoder)?,
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
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += self.0.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.1.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.2.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.3.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.4.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.5.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.6.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.7.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.8.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.9.encode_ext(writer, dedupe_encoder)?;
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
    fn decode(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        Ok((
            A::decode(reader, dedupe_decoder.as_deref_mut())?,
            B::decode(reader, dedupe_decoder.as_deref_mut())?,
            C::decode(reader, dedupe_decoder.as_deref_mut())?,
            D::decode(reader, dedupe_decoder.as_deref_mut())?,
            E::decode(reader, dedupe_decoder.as_deref_mut())?,
            F::decode(reader, dedupe_decoder.as_deref_mut())?,
            G::decode(reader, dedupe_decoder.as_deref_mut())?,
            H::decode(reader, dedupe_decoder.as_deref_mut())?,
            I::decode(reader, dedupe_decoder.as_deref_mut())?,
            J::decode(reader, dedupe_decoder)?,
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
    fn encode_ext(
        &self,
        writer: &mut impl Write,
        mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
    ) -> Result<usize> {
        let mut total_written = 0;
        total_written += self.0.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.1.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.2.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.3.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.4.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.5.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.6.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.7.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.8.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.9.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
        total_written += self.10.encode_ext(writer, dedupe_encoder)?;
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
    fn decode(
        reader: &mut impl Read,
        mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
    ) -> Result<Self> {
        Ok((
            A::decode(reader, dedupe_decoder.as_deref_mut())?,
            B::decode(reader, dedupe_decoder.as_deref_mut())?,
            C::decode(reader, dedupe_decoder.as_deref_mut())?,
            D::decode(reader, dedupe_decoder.as_deref_mut())?,
            E::decode(reader, dedupe_decoder.as_deref_mut())?,
            F::decode(reader, dedupe_decoder.as_deref_mut())?,
            G::decode(reader, dedupe_decoder.as_deref_mut())?,
            H::decode(reader, dedupe_decoder.as_deref_mut())?,
            I::decode(reader, dedupe_decoder.as_deref_mut())?,
            J::decode(reader, dedupe_decoder.as_deref_mut())?,
            K::decode(reader, dedupe_decoder)?,
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

    let written = tuple.encode_ext(&mut buffer, None).unwrap();
    assert_eq!(written, 7);

    let decoded: (u8, u16, u32, u64, u128, usize, i8) =
        Decode::decode(&mut Cursor::new(&buffer[..]), None).unwrap();
    assert_eq!(decoded, tuple);
}
