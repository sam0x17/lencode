use super::prelude::*;
use endian_cast::Endianness;
use ::serde::{
    de,
    ser::{self, Serialize},
};
use core::{fmt::Display, marker::PhantomData};

impl ser::Error for Error {
    fn custom<T: core::fmt::Display>(msg: T) -> Self {
        Error::Serde(msg.to_string())
    }
}

impl de::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Serde(msg.to_string())
    }
}

pub struct Serializer<W: Write, S: Scheme = Lencode> {
    writer: W,
	_s: PhantomData<S>,
}

impl<W: Write> Serializer<W> {
    pub const fn new(writer: W) -> Self {
        Serializer { writer, _s: PhantomData }
    }
}

pub fn to_bytes<T: Serialize, W: Write>(value: &T, writer: W) -> Result<usize> {
    let mut serializer = Serializer::new(writer);
    Ok(value.serialize(&mut serializer)?)
}

impl<'a, W: Write, S: Scheme> ser::Serializer for &'a mut Serializer<W, S> {
    type Ok = usize;
    type Error = Error;
    type SerializeSeq = Self
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

	#[inline(always)]
    fn serialize_bool(self, v: bool) -> Result<usize> {
		v.encode::<S>(&mut self.writer)
    }

	#[inline(always)]
    fn serialize_i8(self, v: i8) -> Result<usize> {
		v.encode::<S>(&mut self.writer)
    }

	#[inline(always)]
    fn serialize_i16(self, v: i16) -> Result<usize> {
		v.encode::<S>(&mut self.writer)
    }

	#[inline(always)]
    fn serialize_i32(self, v: i32) -> Result<usize> {
        v.encode::<S>(&mut self.writer)
    }

	#[inline(always)]
    fn serialize_i64(self, v: i64) -> Result<usize> {
        v.encode::<S>(&mut self.writer)
    }

	#[inline(always)]
    fn serialize_u8(self, v: u8) -> Result<usize> {
        v.encode::<S>(&mut self.writer)
    }

	#[inline(always)]
    fn serialize_u16(self, v: u16) -> Result<usize> {
        v.encode::<S>(&mut self.writer)
    }

	#[inline(always)]
    fn serialize_u32(self, v: u32) -> Result<usize> {
        v.encode::<S>(&mut self.writer)
    }

	#[inline(always)]
    fn serialize_u64(self, v: u64) -> Result<usize> {
        v.encode::<S>(&mut self.writer)
    }

	#[inline(always)]
    fn serialize_f32(self, v: f32) -> Result<usize> {
		// use u32 encoding for f32
		v.to_bits().encode::<S>(&mut self.writer)
    }

	#[inline(always)]
    fn serialize_f64(self, v: f64) -> Result<usize> {
		// use u64 encoding for f64
		v.to_bits().encode::<S>(&mut self.writer)
    }

	#[inline(always)]
    fn serialize_char(self, v: char) -> Result<usize> {
		// use u32 encoding for char
		(v as u32).encode::<S>(&mut self.writer)
    }

	#[inline(always)]
    fn serialize_str(self, v: &str) -> Result<usize> {
		v.encode::<S>(&mut self.writer)
    }

	#[inline(always)]
    fn serialize_bytes(self, v: &[u8]) -> Result<usize> {
		v.encode::<S>(&mut self.writer)
    }

	#[inline(always)]
    fn serialize_none(self) -> Result<usize> {
		S::encode_bool(false, &mut self.writer)
    }

	#[inline(always)]
    fn serialize_some<T>(self, value: &T) -> Result<usize>
    where
        T: ?Sized + Serialize,
    {
                let mut total_written = 0;
                total_written += S::encode_bool(true, &mut self.writer)?;
                total_written += value.serialize(self)?;
                Ok(total_written)
    }

    fn serialize_unit(self) -> Result<usize> {
		Ok(0)
	}

    fn serialize_unit_struct(
        self,
        _name: &'static str,
    ) -> Result<usize> {
		Ok(0)
    }

    fn serialize_unit_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
    ) -> Result<usize> {
        todo!()
    }

    fn serialize_newtype_struct<T>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<usize>
    where
        T: ?Sized + Serialize,
    {
        todo!()
    }

    fn serialize_newtype_variant<T>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<usize>
    where
        T: ?Sized + Serialize,
    {
        todo!()
    }

    fn serialize_seq(
        self,
        len: Option<usize>,
    ) -> core::result::Result<Self::SerializeSeq, Self::Error> {
        todo!()
    }

    fn serialize_tuple(
        self,
        len: usize,
    ) -> core::result::Result<Self::SerializeTuple, Self::Error> {
        todo!()
    }

    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> core::result::Result<Self::SerializeTupleStruct, Self::Error> {
        todo!()
    }

    fn serialize_tuple_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> core::result::Result<Self::SerializeTupleVariant, Self::Error> {
        todo!()
    }

    fn serialize_map(
        self,
        len: Option<usize>,
    ) -> core::result::Result<Self::SerializeMap, Self::Error> {
        todo!()
    }

    fn serialize_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> core::result::Result<Self::SerializeStruct, Self::Error> {
        todo!()
    }

    fn serialize_struct_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> core::result::Result<Self::SerializeStructVariant, Self::Error> {
        todo!()
    }
}
