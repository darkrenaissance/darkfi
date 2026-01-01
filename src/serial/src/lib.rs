/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{
    collections::VecDeque,
    io::{Cursor, Error, Read, Write},
};

#[cfg(feature = "derive")]
pub use darkfi_derive::{SerialDecodable, SerialEncodable};

#[cfg(feature = "async")]
mod async_lib;
#[cfg(feature = "async")]
pub use async_lib::{
    async_trait, deserialize_async, deserialize_async_partial, serialize_async, AsyncDecodable,
    AsyncEncodable, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, FutAsyncReadExt,
    FutAsyncWriteExt,
};

mod endian;
mod types;

/// Data which can be encoded in a consensus-consistent way.
pub trait Encodable {
    /// Encode an object with a well-defined format.
    /// Should only ever error if the underlying `Write` errors.
    /// Returns the number of bytes written on success.
    fn encode<W: Write>(&self, e: &mut W) -> Result<usize, Error>;
}

/// Data which can be decoded in a consensus-consistent way.
pub trait Decodable: Sized {
    fn decode<D: Read>(d: &mut D) -> Result<Self, Error>;
}

/// Encode an object into a vector.
pub fn serialize<T: Encodable + ?Sized>(data: &T) -> Vec<u8> {
    let mut encoder = Vec::new();
    let len = data.encode(&mut encoder).unwrap();
    assert_eq!(len, encoder.len());
    encoder
}

/// Deserialize an object from a vector, but do not error if the entire
/// vector is not consumed.
pub fn deserialize_partial<T: Decodable>(data: &[u8]) -> Result<(T, usize), Error> {
    let mut decoder = Cursor::new(data);
    let rv = Decodable::decode(&mut decoder)?;
    let consumed = decoder.position() as usize;

    Ok((rv, consumed))
}

/// Deserialize an object from a vector.
/// Will error if said deserialization doesn't consume the entire vector.
pub fn deserialize<T: Decodable>(data: &[u8]) -> Result<T, Error> {
    let (rv, consumed) = deserialize_partial(data)?;

    // Fail if data is not consumed entirely.
    if consumed != data.len() {
        return Err(Error::other("Data not consumed fully on deserialization"))
    }

    Ok(rv)
}

/// Extensions of `Write` to encode data as per Bitcoin consensus.
pub trait WriteExt {
    /// Output a 128-bit unsigned int
    fn write_u128(&mut self, v: u128) -> Result<(), Error>;
    /// Output a 64-bit unsigned int
    fn write_u64(&mut self, v: u64) -> Result<(), Error>;
    /// Output a 32-bit unsigned int
    fn write_u32(&mut self, v: u32) -> Result<(), Error>;
    /// Output a 16-bit unsigned int
    fn write_u16(&mut self, v: u16) -> Result<(), Error>;
    /// Output an 8-bit unsigned int
    fn write_u8(&mut self, v: u8) -> Result<(), Error>;

    /// Output a 128-bit signed int
    fn write_i128(&mut self, v: i128) -> Result<(), Error>;
    /// Output a 64-bit signed int
    fn write_i64(&mut self, v: i64) -> Result<(), Error>;
    /// Ouptut a 32-bit signed int
    fn write_i32(&mut self, v: i32) -> Result<(), Error>;
    /// Output a 16-bit signed int
    fn write_i16(&mut self, v: i16) -> Result<(), Error>;
    /// Output an 8-bit signed int
    fn write_i8(&mut self, v: i8) -> Result<(), Error>;

    /// Output a 64-bit floating point int
    fn write_f64(&mut self, v: f64) -> Result<(), Error>;
    /// Output a 32-bit floating point int
    fn write_f32(&mut self, v: f32) -> Result<(), Error>;

    /// Output a boolean
    fn write_bool(&mut self, v: bool) -> Result<(), Error>;

    /// Output a byte slice
    fn write_slice(&mut self, v: &[u8]) -> Result<(), Error>;
}

/// Extensions of `Read` to decode data as per Bitcoin consensus.
pub trait ReadExt {
    /// Read a 128-bit unsigned int
    fn read_u128(&mut self) -> Result<u128, Error>;
    /// Read a 64-bit unsigned int
    fn read_u64(&mut self) -> Result<u64, Error>;
    /// Read a 32-bit unsigned int
    fn read_u32(&mut self) -> Result<u32, Error>;
    /// Read a 16-bit unsigned int
    fn read_u16(&mut self) -> Result<u16, Error>;
    /// Read an 8-bit unsigned int
    fn read_u8(&mut self) -> Result<u8, Error>;

    /// Read a 128-bit signed int
    fn read_i128(&mut self) -> Result<i128, Error>;
    /// Read a 64-bit signed int
    fn read_i64(&mut self) -> Result<i64, Error>;
    /// Ouptut a 32-bit signed int
    fn read_i32(&mut self) -> Result<i32, Error>;
    /// Read a 16-bit signed int
    fn read_i16(&mut self) -> Result<i16, Error>;
    /// Read an 8-bit signed int
    fn read_i8(&mut self) -> Result<i8, Error>;

    /// Read a 64-bit floating point int
    fn read_f64(&mut self) -> Result<f64, Error>;
    /// Read a 32-bit floating point int
    fn read_f32(&mut self) -> Result<f32, Error>;

    /// Read a boolean
    fn read_bool(&mut self) -> Result<bool, Error>;

    /// Read a byte slice
    fn read_slice(&mut self, slice: &mut [u8]) -> Result<(), Error>;
}

macro_rules! encoder_fn {
    ($name:ident, $val_type:ty, $writefn:ident) => {
        #[inline]
        fn $name(&mut self, v: $val_type) -> Result<(), Error> {
            self.write_all(&endian::$writefn(v))
        }
    };
}

macro_rules! decoder_fn {
    ($name:ident, $val_type:ty, $readfn:ident, $byte_len:expr) => {
        #[inline]
        fn $name(&mut self) -> Result<$val_type, Error> {
            assert_eq!(core::mem::size_of::<$val_type>(), $byte_len);
            let mut val = [0; $byte_len];
            self.read_exact(&mut val[..])?;
            Ok(endian::$readfn(&val))
        }
    };
}

impl<W: Write> WriteExt for W {
    encoder_fn!(write_u128, u128, u128_to_array_le);
    encoder_fn!(write_u64, u64, u64_to_array_le);
    encoder_fn!(write_u32, u32, u32_to_array_le);
    encoder_fn!(write_u16, u16, u16_to_array_le);
    encoder_fn!(write_i128, i128, i128_to_array_le);
    encoder_fn!(write_i64, i64, i64_to_array_le);
    encoder_fn!(write_i32, i32, i32_to_array_le);
    encoder_fn!(write_i16, i16, i16_to_array_le);
    encoder_fn!(write_f64, f64, f64_to_array_le);
    encoder_fn!(write_f32, f32, f32_to_array_le);

    #[inline]
    fn write_i8(&mut self, v: i8) -> Result<(), Error> {
        self.write_all(&[v as u8])
    }
    #[inline]
    fn write_u8(&mut self, v: u8) -> Result<(), Error> {
        self.write_all(&[v])
    }
    #[inline]
    fn write_bool(&mut self, v: bool) -> Result<(), Error> {
        self.write_all(&[v as u8])
    }
    #[inline]
    fn write_slice(&mut self, v: &[u8]) -> Result<(), Error> {
        self.write_all(v)
    }
}

impl<R: Read> ReadExt for R {
    decoder_fn!(read_u128, u128, slice_to_u128_le, 16);
    decoder_fn!(read_u64, u64, slice_to_u64_le, 8);
    decoder_fn!(read_u32, u32, slice_to_u32_le, 4);
    decoder_fn!(read_u16, u16, slice_to_u16_le, 2);
    decoder_fn!(read_i128, i128, slice_to_i128_le, 16);
    decoder_fn!(read_i64, i64, slice_to_i64_le, 8);
    decoder_fn!(read_i32, i32, slice_to_i32_le, 4);
    decoder_fn!(read_i16, i16, slice_to_i16_le, 2);
    decoder_fn!(read_f64, f64, slice_to_f64_le, 8);
    decoder_fn!(read_f32, f32, slice_to_f32_le, 4);

    #[inline]
    fn read_u8(&mut self) -> Result<u8, Error> {
        let mut slice = [0u8; 1];
        self.read_exact(&mut slice)?;
        Ok(slice[0])
    }
    #[inline]
    fn read_i8(&mut self) -> Result<i8, Error> {
        let mut slice = [0u8; 1];
        self.read_exact(&mut slice)?;
        Ok(slice[0] as i8)
    }
    #[inline]
    fn read_bool(&mut self) -> Result<bool, Error> {
        ReadExt::read_i8(self).map(|bit| bit != 0)
    }
    #[inline]
    fn read_slice(&mut self, slice: &mut [u8]) -> Result<(), Error> {
        self.read_exact(slice)
    }
}

macro_rules! impl_int_encodable {
    ($ty:ident, $meth_dec:ident, $meth_enc:ident) => {
        impl Decodable for $ty {
            #[inline]
            fn decode<D: Read>(d: &mut D) -> Result<Self, Error> {
                ReadExt::$meth_dec(d).map($ty::from_le)
            }
        }

        impl Encodable for $ty {
            #[inline]
            fn encode<S: WriteExt>(&self, s: &mut S) -> Result<usize, Error> {
                s.$meth_enc(self.to_le())?;
                Ok(core::mem::size_of::<$ty>())
            }
        }
    };
}

impl_int_encodable!(u8, read_u8, write_u8);
impl_int_encodable!(u16, read_u16, write_u16);
impl_int_encodable!(u32, read_u32, write_u32);
impl_int_encodable!(u64, read_u64, write_u64);
impl_int_encodable!(u128, read_u128, write_u128);

impl_int_encodable!(i8, read_i8, write_i8);
impl_int_encodable!(i16, read_i16, write_i16);
impl_int_encodable!(i32, read_i32, write_i32);
impl_int_encodable!(i64, read_i64, write_i64);
impl_int_encodable!(i128, read_i128, write_i128);

/// Variable-integer encoding.
///
/// Integer can be encoded depending on the represented value to save space.
/// Variable length integers always precede an array/vector of a type of data
/// that may vary in length. Longer numbers are encoded in little endian.
///
/// | Value         | Storage length | Format                              |
/// |---------------|----------------|-------------------------------------|
/// | <= 0xfc       | 1              | u8                                  |
/// | <= 0xffff     | 3              | `0xfd` followed by `value` as `u16` |
/// | <= 0xffffffff | 5              | `0xfe` followed by `value` as `u32` |
/// | -             | 9              | `0xff` followed by `value` as `u64` |
///
/// See also [Bitcoin variable length integers](https://en.bitcoin.it/wiki/Protocol_documentation#Variable_length_integer).
#[derive(Debug, PartialEq, Eq)]
pub struct VarInt(pub u64);

impl VarInt {
    /// Gets the length of this `VarInt` when encoded.
    /// Returns:
    /// * 1 for 0..0xFC
    /// * 3 for 0xFD..(2^16-1)
    /// * 5 for 0x10000..(2^32-1)
    /// * 9 otherwise
    #[inline]
    pub fn length(&self) -> usize {
        match self.0 {
            0..=0xFC => 1,
            0xFD..=0xFFFF => 3,
            0x10000..=0xFFFFFFFF => 5,
            _ => 9,
        }
    }
}

impl Encodable for VarInt {
    #[inline]
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize, Error> {
        match self.0 {
            0..=0xFC => {
                (self.0 as u8).encode(s)?;
                Ok(1)
            }

            0xFD..=0xFFFF => {
                s.write_u8(0xFD)?;
                (self.0 as u16).encode(s)?;
                Ok(3)
            }

            0x10000..=0xFFFFFFFF => {
                s.write_u8(0xFE)?;
                (self.0 as u32).encode(s)?;
                Ok(5)
            }

            _ => {
                s.write_u8(0xFF)?;
                self.0.encode(s)?;
                Ok(9)
            }
        }
    }
}

impl Decodable for VarInt {
    #[inline]
    fn decode<D: Read>(d: &mut D) -> Result<Self, Error> {
        let n = ReadExt::read_u8(d)?;
        match n {
            0xFF => {
                let x = ReadExt::read_u64(d)?;
                if x < 0x100000000 {
                    return Err(Error::other("Non-minimal VarInt"))
                }
                Ok(VarInt(x))
            }

            0xFE => {
                let x = ReadExt::read_u32(d)?;
                if x < 0x10000 {
                    return Err(Error::other("Non-minimal VarInt"))
                }
                Ok(VarInt(x as u64))
            }

            0xFD => {
                let x = ReadExt::read_u16(d)?;
                if x < 0xFD {
                    return Err(Error::other("Non-minimal VarInt"))
                }
                Ok(VarInt(x as u64))
            }

            n => Ok(VarInt(n as u64)),
        }
    }
}

macro_rules! tuple_encode {
    ($($x:ident),*) => (
        impl<$($x: Encodable),*> Encodable for ($($x),*) {
            #[inline]
            #[allow(non_snake_case)]
            fn encode<S: Write>(&self, s: &mut S) -> Result<usize, Error> {
                let &($(ref $x),*) = self;
                let mut len = 0;
                $(len += $x.encode(s)?;)*
                Ok(len)
            }
        }

        impl<$($x: Decodable),*> Decodable for ($($x),*) {
            #[inline]
            #[allow(non_snake_case)]
            fn decode<D: Read>(d: &mut D) -> Result<Self, Error> {
                Ok(($({let $x = Decodable::decode(d)?; $x }),*))
            }
        }
    )
}

tuple_encode!(T0, T1);
tuple_encode!(T0, T1, T2);
tuple_encode!(T0, T1, T2, T3);
tuple_encode!(T0, T1, T2, T3, T4);
tuple_encode!(T0, T1, T2, T3, T4, T5);
tuple_encode!(T0, T1, T2, T3, T4, T5, T6);
tuple_encode!(T0, T1, T2, T3, T4, T5, T6, T7);

/// Encode a dynamic set of arguments to a buffer.
#[macro_export]
macro_rules! encode_payload {
    ($buf:expr, $($args:expr),*) => {{ $( $args.encode($buf)?;)* }}
}

// Implementations for some primitive types.
impl Encodable for usize {
    #[inline]
    fn encode<S: WriteExt>(&self, s: &mut S) -> Result<usize, Error> {
        s.write_u64(*self as u64)?;
        Ok(8)
    }
}

impl Decodable for usize {
    #[inline]
    fn decode<D: Read>(d: &mut D) -> Result<Self, Error> {
        Ok(ReadExt::read_u64(d)? as usize)
    }
}

impl Encodable for f64 {
    #[inline]
    fn encode<S: WriteExt>(&self, s: &mut S) -> Result<usize, Error> {
        s.write_f64(*self)?;
        Ok(core::mem::size_of::<f64>())
    }
}

impl Decodable for f64 {
    #[inline]
    fn decode<D: Read>(d: &mut D) -> Result<Self, Error> {
        ReadExt::read_f64(d)
    }
}

impl Encodable for f32 {
    #[inline]
    fn encode<S: WriteExt>(&self, s: &mut S) -> Result<usize, Error> {
        s.write_f32(*self)?;
        Ok(core::mem::size_of::<f32>())
    }
}

impl Decodable for f32 {
    #[inline]
    fn decode<D: Read>(d: &mut D) -> Result<Self, Error> {
        ReadExt::read_f32(d)
    }
}

impl Encodable for bool {
    #[inline]
    fn encode<S: WriteExt>(&self, s: &mut S) -> Result<usize, Error> {
        s.write_bool(*self)?;
        Ok(1)
    }
}

impl Decodable for bool {
    #[inline]
    fn decode<D: Read>(d: &mut D) -> Result<Self, Error> {
        ReadExt::read_bool(d)
    }
}

impl<T: Encodable> Encodable for Vec<T> {
    #[inline]
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize, Error> {
        let mut len = 0;
        len += VarInt(self.len() as u64).encode(s)?;
        for val in self {
            len += val.encode(s)?;
        }
        Ok(len)
    }
}

impl<T: Decodable> Decodable for Vec<T> {
    #[inline]
    fn decode<D: Read>(d: &mut D) -> Result<Self, Error> {
        let len = VarInt::decode(d)?.0;
        let mut ret = Vec::new();
        ret.try_reserve(len as usize).map_err(|_| std::io::ErrorKind::InvalidData)?;
        for _ in 0..len {
            ret.push(Decodable::decode(d)?);
        }
        Ok(ret)
    }
}

impl<T: Encodable> Encodable for VecDeque<T> {
    #[inline]
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize, Error> {
        let mut len = 0;
        len += VarInt(self.len() as u64).encode(s)?;
        for val in self {
            len += val.encode(s)?;
        }
        Ok(len)
    }
}

impl<T: Decodable> Decodable for VecDeque<T> {
    #[inline]
    fn decode<D: Read>(d: &mut D) -> Result<Self, Error> {
        let len = VarInt::decode(d)?.0;
        let mut ret = VecDeque::new();
        ret.try_reserve(len as usize).map_err(|_| std::io::ErrorKind::InvalidData)?;
        for _ in 0..len {
            ret.push_back(Decodable::decode(d)?);
        }
        Ok(ret)
    }
}

impl<T: Encodable> Encodable for Option<T> {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize, Error> {
        let mut len = 0;
        if let Some(v) = self {
            len += true.encode(s)?;
            len += v.encode(s)?;
        } else {
            len += false.encode(s)?;
        }
        Ok(len)
    }
}

impl<T: Decodable> Decodable for Option<T> {
    fn decode<D: Read>(d: &mut D) -> Result<Self, Error> {
        let valid: bool = Decodable::decode(d)?;
        let val = if valid { Some(Decodable::decode(d)?) } else { None };
        Ok(val)
    }
}

impl<T, const N: usize> Encodable for [T; N]
where
    T: Encodable,
{
    #[inline]
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize, Error> {
        let mut len = 0;
        for elem in self.iter() {
            len += elem.encode(s)?;
        }
        Ok(len)
    }
}

impl<T, const N: usize> Decodable for [T; N]
where
    T: Decodable + core::fmt::Debug,
{
    #[inline]
    fn decode<D: Read>(d: &mut D) -> Result<Self, Error> {
        let mut ret = vec![];
        for _ in 0..N {
            ret.push(Decodable::decode(d)?);
        }

        Ok(ret.try_into().unwrap())
    }
}

impl Encodable for String {
    #[inline]
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize, Error> {
        let b = self.as_bytes();
        let b_len = b.len();
        let vi_len = VarInt(b_len as u64).encode(s)?;
        s.write_slice(b)?;
        Ok(vi_len + b_len)
    }
}

impl Encodable for &str {
    #[inline]
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize, Error> {
        let b = self.as_bytes();
        let b_len = b.len();
        let vi_len = VarInt(b_len as u64).encode(s)?;
        s.write_slice(b)?;
        Ok(vi_len + b_len)
    }
}

impl Decodable for String {
    #[inline]
    fn decode<D: Read>(d: &mut D) -> Result<String, Error> {
        match String::from_utf8(Decodable::decode(d)?) {
            Ok(v) => Ok(v),
            Err(_) => Err(Error::other("Invalid UTF-8 for string")),
        }
    }
}

/*
impl Encodable for Cow<'static, str> {
    #[inline]
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        let b = self.as_bytes();
        let b_len = b.len();
        let vi_len = VarInt(b_len as u64).encode(&mut s)?;
        s.write_slice(b)?;
        Ok(vi_len + b_len)
    }
}

impl Decodable for Cow<'static, str> {
    #[inline]
    fn decode<D: Read>(d: D) -> Result<Cow<'static, str>, Error> {
        match String::from_utf8(Decodable::decode(d)?) {
            Ok(v) => v.map(Cow::Owned),
            Err(_) => Err(Error::other("Invalid UTF-8 for string")),
        }
    }
}
*/

#[cfg(test)]
mod tests {
    use super::{endian::*, *};
    use futures_lite::AsyncWriteExt;

    #[test]
    fn serialize_int_test() {
        // bool
        assert_eq!(serialize(&false), vec![0u8]);
        assert_eq!(serialize(&true), vec![1u8]);
        // u8
        assert_eq!(serialize(&1u8), vec![1u8]);
        assert_eq!(serialize(&0u8), vec![0u8]);
        assert_eq!(serialize(&255u8), vec![255u8]);
        // u16
        assert_eq!(serialize(&1u16), vec![1u8, 0]);
        assert_eq!(serialize(&256u16), vec![0u8, 1]);
        assert_eq!(serialize(&5000u16), vec![136u8, 19]);
        // u32
        assert_eq!(serialize(&1u32), vec![1u8, 0, 0, 0]);
        assert_eq!(serialize(&256u32), vec![0u8, 1, 0, 0]);
        assert_eq!(serialize(&5000u32), vec![136u8, 19, 0, 0]);
        assert_eq!(serialize(&500000u32), vec![32u8, 161, 7, 0]);
        assert_eq!(serialize(&168430090u32), vec![10u8, 10, 10, 10]);
        // i32
        assert_eq!(serialize(&-1i32), vec![255u8, 255, 255, 255]);
        assert_eq!(serialize(&-256i32), vec![0u8, 255, 255, 255]);
        assert_eq!(serialize(&-5000i32), vec![120u8, 236, 255, 255]);
        assert_eq!(serialize(&-500000i32), vec![224u8, 94, 248, 255]);
        assert_eq!(serialize(&-168430090i32), vec![246u8, 245, 245, 245]);
        assert_eq!(serialize(&1i32), vec![1u8, 0, 0, 0]);
        assert_eq!(serialize(&256i32), vec![0u8, 1, 0, 0]);
        assert_eq!(serialize(&5000i32), vec![136u8, 19, 0, 0]);
        assert_eq!(serialize(&500000i32), vec![32u8, 161, 7, 0]);
        assert_eq!(serialize(&168430090i32), vec![10u8, 10, 10, 10]);
        // u64
        assert_eq!(serialize(&1u64), vec![1u8, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(serialize(&256u64), vec![0u8, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(serialize(&5000u64), vec![136u8, 19, 0, 0, 0, 0, 0, 0]);
        assert_eq!(serialize(&500000u64), vec![32u8, 161, 7, 0, 0, 0, 0, 0]);
        assert_eq!(serialize(&723401728380766730u64), vec![10u8, 10, 10, 10, 10, 10, 10, 10]);
        // i64
        assert_eq!(serialize(&-1i64), vec![255u8, 255, 255, 255, 255, 255, 255, 255]);
        assert_eq!(serialize(&-256i64), vec![0u8, 255, 255, 255, 255, 255, 255, 255]);
        assert_eq!(serialize(&-5000i64), vec![120u8, 236, 255, 255, 255, 255, 255, 255]);
        assert_eq!(serialize(&-500000i64), vec![224u8, 94, 248, 255, 255, 255, 255, 255]);
        assert_eq!(
            serialize(&-723401728380766730i64),
            vec![246u8, 245, 245, 245, 245, 245, 245, 245]
        );
        assert_eq!(serialize(&1i64), vec![1u8, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(serialize(&256i64), vec![0u8, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(serialize(&5000i64), vec![136u8, 19, 0, 0, 0, 0, 0, 0]);
        assert_eq!(serialize(&500000i64), vec![32u8, 161, 7, 0, 0, 0, 0, 0]);
        assert_eq!(serialize(&723401728380766730i64), vec![10u8, 10, 10, 10, 10, 10, 10, 10]);
    }

    #[test]
    fn serialize_float_test() {
        // f64
        assert_eq!(serialize(&1.5f64), vec![0u8, 0, 0, 0, 0, 0, 248, 63]);
        assert_eq!(serialize(&256.7f64), vec![51u8, 51, 51, 51, 51, 11, 112, 64]);
        assert_eq!(serialize(&5000.21f64), vec![41u8, 92, 143, 194, 53, 136, 179, 64]);
        assert_eq!(serialize(&500000.314f64), vec![76u8, 55, 137, 65, 129, 132, 30, 65]);
        assert_eq!(serialize(&1102021.1102021f64), vec![111u8, 52, 54, 28, 197, 208, 48, 65]);
        assert_eq!(
            serialize(&723401728380766730.894612f64),
            vec![20u8, 20, 20, 20, 20, 20, 164, 67]
        );
        assert_eq!(serialize(&-1.5f64), vec![0u8, 0, 0, 0, 0, 0, 248, 191]);
        assert_eq!(serialize(&-256.7f64), vec![51u8, 51, 51, 51, 51, 11, 112, 192]);
        assert_eq!(serialize(&-5000.21f64), vec![41u8, 92, 143, 194, 53, 136, 179, 192]);
        assert_eq!(serialize(&-500000.314f64), vec![76u8, 55, 137, 65, 129, 132, 30, 193]);
        assert_eq!(serialize(&-1102021.1102021f64), vec![111u8, 52, 54, 28, 197, 208, 48, 193]);
        assert_eq!(
            serialize(&-723401728380766730.894612f64),
            vec![20u8, 20, 20, 20, 20, 20, 164, 195]
        );
        // f32
        assert_eq!(serialize(&1.5f32), vec![0u8, 0, 192, 63]);
        assert_eq!(serialize(&256.7f32), vec![154u8, 89, 128, 67]);
        assert_eq!(serialize(&5000.21f32), vec![174u8, 65, 156, 69]);
        assert_eq!(serialize(&500000.3f32), vec![10u8, 36, 244, 72]);
        assert_eq!(serialize(&1102021.1f32), vec![41u8, 134, 134, 73]);
        assert_eq!(serialize(&72340172838076673.9f32), vec![129u8, 128, 128, 91]);
        assert_eq!(serialize(&-1.5f32), vec![0u8, 0, 192, 191]);
        assert_eq!(serialize(&-256.7f32), vec![154u8, 89, 128, 195]);
        assert_eq!(serialize(&-5000.21f32), vec![174u8, 65, 156, 197]);
        assert_eq!(serialize(&-500000.3f32), vec![10u8, 36, 244, 200]);
        assert_eq!(serialize(&-1102021.1f32), vec![41u8, 134, 134, 201]);
        assert_eq!(serialize(&-72340172838076673.9f32), vec![129u8, 128, 128, 219]);
    }

    #[test]
    fn serialize_varint_test() {
        assert_eq!(serialize(&VarInt(10)), vec![10u8]);
        assert_eq!(serialize(&VarInt(0xFC)), vec![0xFCu8]);
        assert_eq!(serialize(&VarInt(0xFD)), vec![0xFDu8, 0xFD, 0]);
        assert_eq!(serialize(&VarInt(0xFFF)), vec![0xFDu8, 0xFF, 0xF]);
        assert_eq!(serialize(&VarInt(0xF0F0F0F)), vec![0xFEu8, 0xF, 0xF, 0xF, 0xF]);
        assert_eq!(
            serialize(&VarInt(0xF0F0F0F0F0E0)),
            vec![0xFFu8, 0xE0, 0xF0, 0xF0, 0xF0, 0xF0, 0xF0, 0, 0]
        );
        assert_eq!(
            test_varint_encode(0xFF, &u64_to_array_le(0x100000000)).unwrap(),
            VarInt(0x100000000)
        );
        assert_eq!(test_varint_encode(0xFE, &u64_to_array_le(0x10000)).unwrap(), VarInt(0x10000));
        assert_eq!(test_varint_encode(0xFD, &u64_to_array_le(0xFD)).unwrap(), VarInt(0xFD));

        // Test that length calc is working correctly
        test_varint_len(VarInt(0), 1);
        test_varint_len(VarInt(0xFC), 1);
        test_varint_len(VarInt(0xFD), 3);
        test_varint_len(VarInt(0xFFFF), 3);
        test_varint_len(VarInt(0x10000), 5);
        test_varint_len(VarInt(0xFFFFFFFF), 5);
        test_varint_len(VarInt(0xFFFFFFFF + 1), 9);
        test_varint_len(VarInt(u64::MAX), 9);
    }

    fn test_varint_len(varint: VarInt, expected: usize) {
        let mut encoder = Cursor::new(vec![]);
        assert_eq!(varint.encode(&mut encoder).unwrap(), expected);
        assert_eq!(varint.length(), expected);
    }

    fn test_varint_encode(n: u8, x: &[u8]) -> Result<VarInt, Error> {
        let mut input = [0u8; 9];
        input[0] = n;
        input[1..x.len() + 1].copy_from_slice(x);
        deserialize_partial::<VarInt>(&input).map(|t| t.0)
    }

    #[test]
    fn deserialize_nonminimal_vec() {
        // Check the edges for variant int
        assert!(test_varint_encode(0xFF, &u64_to_array_le(0x100000000 - 1)).is_err());
        assert!(test_varint_encode(0xFE, &u32_to_array_le(0x10000 - 1)).is_err());
        assert!(test_varint_encode(0xFD, &u16_to_array_le(0xFD - 1)).is_err());
        assert!(deserialize::<Vec<u8>>(&[0xfd, 0x00, 0x00]).is_err());
        assert!(deserialize::<Vec<u8>>(&[0xfd, 0xfc, 0x00]).is_err());
        assert!(deserialize::<Vec<u8>>(&[0xfd, 0xfc, 0x00]).is_err());
        assert!(deserialize::<Vec<u8>>(&[0xfe, 0xff, 0x00, 0x00, 0x00]).is_err());
        assert!(deserialize::<Vec<u8>>(&[0xfe, 0xff, 0xff, 0x00, 0x00]).is_err());
        assert!(deserialize::<Vec<u8>>(&[0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
            .is_err());
        assert!(deserialize::<Vec<u8>>(&[0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00])
            .is_err());

        let mut vec_256 = vec![0; 259];
        vec_256[0] = 0xfd;
        vec_256[1] = 0x00;
        vec_256[2] = 0x01;
        assert!(deserialize::<Vec<u8>>(&vec_256).is_ok());

        let mut vec_253 = vec![0; 256];
        vec_253[0] = 0xfd;
        vec_253[1] = 0xfd;
        vec_253[2] = 0x00;
        assert!(deserialize::<Vec<u8>>(&vec_253).is_ok());
    }

    #[test]
    fn serialize_vector_test() {
        assert_eq!(serialize(&vec![1u8, 2, 3]), vec![3u8, 1, 2, 3]);
        assert_eq!(serialize(&vec![1u16, 2u16]), vec![2u8, 1, 0, 2, 0]);
        assert_eq!(serialize(&vec![256u16, 5000u16]), vec![2u8, 0, 1, 136, 19]);
        assert_eq!(
            serialize(&vec![1u32, 256u32, 5000u32]),
            vec![3u8, 1, 0, 0, 0, 0, 1, 0, 0, 136, 19, 0, 0]
        );
        assert_eq!(
            serialize(&vec![1u64, 256u64, 5000u64, 500000u64]),
            vec![
                4u8, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 136, 19, 0, 0, 0, 0, 0, 0, 32,
                161, 7, 0, 0, 0, 0, 0
            ]
        );
        assert_eq!(serialize(&vec![-1i8]), vec![1u8, 255]);
        assert_eq!(serialize(&vec![1i8, -2i8, -3i8]), vec![3u8, 1, 254, 253]);
        assert_eq!(serialize(&vec![-1i32]), vec![1u8, 255, 255, 255, 255]);
        assert_eq!(serialize(&vec![-1i32, -256]), vec![2u8, 255, 255, 255, 255, 0, 255, 255, 255]);
        assert_eq!(
            serialize(&vec![-1i32, -2i32, -3i32]),
            vec![3u8, 255, 255, 255, 255, 254, 255, 255, 255, 253, 255, 255, 255]
        );
        assert_eq!(
            serialize(&vec![-1i64, -256i64, -5000i64, -500000i64]),
            vec![
                4u8, 255, 255, 255, 255, 255, 255, 255, 255, 0, 255, 255, 255, 255, 255, 255, 255,
                120, 236, 255, 255, 255, 255, 255, 255, 224, 94, 248, 255, 255, 255, 255, 255
            ]
        );
    }

    #[test]
    fn serialize_strbuf_test() {
        assert_eq!(serialize(&"Andrew".to_string()), vec![6u8, 0x41, 0x6e, 0x64, 0x72, 0x65, 0x77]);
    }

    #[test]
    fn deserialize_int_test() {
        // bool
        assert!((deserialize(&[58u8, 0]) as Result<bool, Error>).is_err());
        assert_eq!(deserialize(&[58u8]).ok(), Some(true));
        assert_eq!(deserialize(&[1u8]).ok(), Some(true));
        assert_eq!(deserialize(&[0u8]).ok(), Some(false));
        assert!((deserialize(&[0u8, 1]) as Result<bool, Error>).is_err());

        // u8
        assert_eq!(deserialize(&[58u8]).ok(), Some(58u8));

        // u16
        assert_eq!(deserialize(&[0x01u8, 0x02]).ok(), Some(0x0201u16));
        assert_eq!(deserialize(&[0xABu8, 0xCD]).ok(), Some(0xCDABu16));
        assert_eq!(deserialize(&[0xA0u8, 0x0D]).ok(), Some(0xDA0u16));
        let failure16: Result<u16, Error> = deserialize(&[1u8]);
        assert!(failure16.is_err());

        // u32
        assert_eq!(deserialize(&[0xABu8, 0xCD, 0, 0]).ok(), Some(0xCDABu32));
        assert_eq!(deserialize(&[0xA0u8, 0x0D, 0xAB, 0xCD]).ok(), Some(0xCDAB0DA0u32));
        let failure32: Result<u32, Error> = deserialize(&[1u8, 2, 3]);
        assert!(failure32.is_err());

        assert_eq!(deserialize(&[0x78u8, 0xec, 0xff, 0xff]).ok(), Some(-5000i32));
        assert_eq!(deserialize(&[0xABu8, 0xCD, 0, 0]).ok(), Some(0xCDABi32));
        assert_eq!(deserialize(&[0xA0u8, 0x0D, 0xAB, 0x2D]).ok(), Some(0x2DAB0DA0i32));
        let failurei32: Result<i32, Error> = deserialize(&[1u8, 2, 3]);
        assert!(failurei32.is_err());

        // u64
        assert_eq!(deserialize(&[0xABu8, 0xCD, 0, 0, 0, 0, 0, 0]).ok(), Some(0xCDABu64));
        assert_eq!(
            deserialize(&[0xA0u8, 0x0D, 0xAB, 0xCD, 0x99, 0, 0, 0x99]).ok(),
            Some(0x99000099CDAB0DA0u64)
        );
        let failure64: Result<u64, Error> = deserialize(&[1u8, 2, 3, 4, 5, 6, 7]);
        assert!(failure64.is_err());
        assert_eq!(
            deserialize(&[0xe0, 0x5e, 0xf8, 0xff, 0xff, 0xff, 0xff, 0xff]).ok(),
            Some(-500000i64)
        );
        assert_eq!(deserialize(&[0xABu8, 0xCD, 0, 0, 0, 0, 0, 0]).ok(), Some(0xCDABi64));
        assert_eq!(
            deserialize(&[0xA0u8, 0x0D, 0xAB, 0xCD, 0x99, 0, 0, 0x99]).ok(),
            Some(-0x66ffff663254f260i64)
        );
        let failurei64: Result<i64, Error> = deserialize(&[1u8, 2, 3, 4, 5, 6, 7]);
        assert!(failurei64.is_err());
    }
    #[test]
    fn deserialize_vec_test() {
        assert_eq!(deserialize(&[3u8, 2, 3, 4]).ok(), Some(vec![2u8, 3, 4]));
        assert!((deserialize(&[4u8, 2, 3, 4, 5, 6]) as Result<Vec<u8>, Error>).is_err());
    }

    #[test]
    fn deserialize_strbuf_test() {
        assert_eq!(
            deserialize(&[6u8, 0x41, 0x6e, 0x64, 0x72, 0x65, 0x77]).ok(),
            Some("Andrew".to_string())
        );
    }

    #[test]
    fn encode_payload_test() -> Result<(), Error> {
        let mut i32_buf1 = vec![];
        let mut i32_buf2 = vec![];
        1_i32.encode(&mut i32_buf1)?;
        2_i32.encode(&mut i32_buf2)?;

        let mut string_buf = vec![];
        b"Hello World".encode(&mut string_buf)?;

        /*
        eprintln!("{:?}", i32_buf1);
        eprintln!("{:?}", i32_buf2);
        eprintln!("{:?}", string_buf);
        */

        let mut buf = vec![];
        let mut buf_verify = vec![];
        buf_verify.extend_from_slice(&i32_buf1);
        buf_verify.extend_from_slice(&i32_buf2);
        buf_verify.extend_from_slice(&string_buf);

        encode_payload!(&mut buf, 1_i32, 2_i32, b"Hello World");
        assert_eq!(buf, buf_verify);

        let mut f64_buf = vec![];
        let mut i64_buf = vec![];
        let mut bool_buf = vec![];
        let mut u32_buf = vec![];
        let mut array_buf = vec![];
        1.5f64.encode(&mut f64_buf)?;
        (-1i64).encode(&mut i64_buf)?;
        true.encode(&mut bool_buf)?;
        0x10000_u32.encode(&mut u32_buf)?;
        [0xfe, 0xff, 0x00, 0x00, 0x00].encode(&mut array_buf)?;

        /*
        eprintln!("{:?}", f64_buf);
        eprintln!("{:?}", i64_buf);
        eprintln!("{:?}", bool_buf);
        eprintln!("{:?}", u32_buf);
        eprintln!("{:?}", array_buf);
        */

        let mut buf = vec![];
        let mut buf_verify = vec![];
        buf_verify.extend_from_slice(&f64_buf);
        buf_verify.extend_from_slice(&i64_buf);
        buf_verify.extend_from_slice(&bool_buf);
        buf_verify.extend_from_slice(&u32_buf);
        buf_verify.extend_from_slice(&array_buf);
        encode_payload!(&mut buf, 1.5f64, -1i64, true, 0x10000_u32, [0xfe, 0xff, 0x00, 0x00, 0x00]);
        assert_eq!(buf, buf_verify);

        Ok(())
    }

    #[derive(Debug, PartialEq, SerialEncodable, SerialDecodable)]
    enum TestEnum0 {
        First,
        Second,
        Third,
    }

    #[derive(Debug, PartialEq, SerialEncodable, SerialDecodable)]
    enum TestEnum1 {
        First = 0x01,
        Second = 0x03,
        Third = 0xf1,
        Fourth = 0xfe,
    }

    #[test]
    fn derive_serialize_deserialize_enum() {
        let first = serialize(&TestEnum0::First);
        let second = serialize(&TestEnum0::Second);
        let third = serialize(&TestEnum0::Third);
        assert_eq!(deserialize::<TestEnum0>(&first).unwrap(), TestEnum0::First);
        assert_eq!(deserialize::<TestEnum0>(&second).unwrap(), TestEnum0::Second);
        assert_eq!(deserialize::<TestEnum0>(&third).unwrap(), TestEnum0::Third);

        let first = serialize(&TestEnum1::First);
        let second = serialize(&TestEnum1::Second);
        let third = serialize(&TestEnum1::Third);
        let fourth = serialize(&TestEnum1::Fourth);
        assert_eq!(first, [0x01]);
        assert_eq!(second, [0x03]);
        assert_eq!(third, [0xf1]);
        assert_eq!(fourth, [0xfe]);
    }

    #[derive(Debug, PartialEq, SerialEncodable, SerialDecodable)]
    struct TestStruct0 {
        foo: u64,
        bar: bool,
        baz: String,
    }

    #[derive(Debug, PartialEq, SerialEncodable, SerialDecodable)]
    struct TestStruct1(String);

    #[test]
    fn derive_serialize_deserialize_struct() {
        let foo = 44;
        let bar = true;
        let baz = String::from("foobarbaz");
        let ts0 = TestStruct0 { foo, bar, baz: baz.clone() };
        let ts0_s = serialize(&ts0);
        let ts0_n = deserialize::<TestStruct0>(&ts0_s).unwrap();
        assert_eq!(foo, ts0_n.foo);
        assert_eq!(bar, ts0_n.bar);
        assert_eq!(baz, ts0_n.baz);
        assert_eq!(ts0, ts0_n);
        assert_eq!(ts0_n, TestStruct0 { foo, bar, baz: baz.clone() });

        let ts1 = TestStruct1(baz.clone());
        let ts1_s = serialize(&ts1);
        let ts1_n = deserialize::<TestStruct1>(&ts1_s).unwrap();
        assert_eq!(ts1, ts1_n);
        assert_eq!(ts1_n, TestStruct1(baz));
    }
}
