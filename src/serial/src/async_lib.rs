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
    io::{Error, ErrorKind, Result},
};

pub use async_trait::async_trait;
pub use futures_lite::{
    io::Cursor, AsyncRead, AsyncReadExt as FutAsyncReadExt, AsyncWrite,
    AsyncWriteExt as FutAsyncWriteExt,
};

use crate::{endian, VarInt};

/// Data which can asynchronously be encoded in a consensus-consistent way.
#[async_trait]
pub trait AsyncEncodable {
    /// Asynchronously encode an object with a well-defined format.
    /// Should only ever error if the underlying `AsyncWrite` errors.
    /// Returns the number of bytes written on success.
    async fn encode_async<W: AsyncWrite + Unpin + Send>(&self, w: &mut W) -> Result<usize>;
}

/// Data which can asynchronously be decoded in a consensus-consistent way.
#[async_trait]
pub trait AsyncDecodable: Sized {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self>;
}

/// Asynchronously encode an object into a vector.
pub async fn serialize_async<T: AsyncEncodable + ?Sized>(data: &T) -> Vec<u8> {
    let mut encoder = Vec::new();
    let len = data.encode_async(&mut encoder).await.unwrap();
    assert_eq!(len, encoder.len());
    encoder
}

/// Asynchronously deserialize an object from a vector, but do not error if the
/// entire vector is not consumed.
pub async fn deserialize_async_partial<T: AsyncDecodable>(data: &[u8]) -> Result<(T, usize)> {
    let mut decoder = Cursor::new(data);
    let rv = AsyncDecodable::decode_async(&mut decoder).await?;
    let consumed = decoder.position() as usize;

    Ok((rv, consumed))
}

/// Asynchronously deserialize an object from a vector.
/// Will error if said deserialization doesn't consume the entire vector.
pub async fn deserialize_async<T: AsyncDecodable>(data: &[u8]) -> Result<T> {
    let (rv, consumed) = deserialize_async_partial(data).await?;

    // Fail if data is not consumed entirely.
    if consumed != data.len() {
        return Err(Error::new(ErrorKind::Other, "Data not consumed fully on deserialization"))
    }

    Ok(rv)
}

/// Extensions of `AsyncWrite` to encode data as per Bitcoin consensus.
#[async_trait]
pub trait AsyncWriteExt {
    /// Output a 128-bit unsigned int
    async fn write_u128_async(&mut self, v: u128) -> Result<()>;
    /// Output a 64-bit unsigned int
    async fn write_u64_async(&mut self, v: u64) -> Result<()>;
    /// Output a 32-bit unsigned int
    async fn write_u32_async(&mut self, v: u32) -> Result<()>;
    /// Output a 16-bit unsigned int
    async fn write_u16_async(&mut self, v: u16) -> Result<()>;
    /// Output an 8-bit unsigned int
    async fn write_u8_async(&mut self, v: u8) -> Result<()>;

    /// Output a 128-bit signed int
    async fn write_i128_async(&mut self, v: i128) -> Result<()>;
    /// Output a 64-bit signed int
    async fn write_i64_async(&mut self, v: i64) -> Result<()>;
    /// Ouptut a 32-bit signed int
    async fn write_i32_async(&mut self, v: i32) -> Result<()>;
    /// Output a 16-bit signed int
    async fn write_i16_async(&mut self, v: i16) -> Result<()>;
    /// Output an 8-bit signed int
    async fn write_i8_async(&mut self, v: i8) -> Result<()>;

    /// Output a 64-bit floating point int
    async fn write_f64_async(&mut self, v: f64) -> Result<()>;
    /// Output a 32-bit floating point int
    async fn write_f32_async(&mut self, v: f32) -> Result<()>;

    /// Output a boolean
    async fn write_bool_async(&mut self, v: bool) -> Result<()>;

    /// Output a byte slice
    async fn write_slice_async(&mut self, v: &[u8]) -> Result<()>;
}

/// Extensions of `AsyncRead` to decode data as per Bitcoin consensus.
#[async_trait]
pub trait AsyncReadExt {
    /// Read a 128-bit unsigned int
    async fn read_u128_async(&mut self) -> Result<u128>;
    /// Read a 64-bit unsigned int
    async fn read_u64_async(&mut self) -> Result<u64>;
    /// Read a 32-bit unsigned int
    async fn read_u32_async(&mut self) -> Result<u32>;
    /// Read a 16-bit unsigned int
    async fn read_u16_async(&mut self) -> Result<u16>;
    /// Read an 8-bit unsigned int
    async fn read_u8_async(&mut self) -> Result<u8>;

    /// Read a 128-bit signed int
    async fn read_i128_async(&mut self) -> Result<i128>;
    /// Read a 64-bit signed int
    async fn read_i64_async(&mut self) -> Result<i64>;
    /// Ouptut a 32-bit signed int
    async fn read_i32_async(&mut self) -> Result<i32>;
    /// Read a 16-bit signed int
    async fn read_i16_async(&mut self) -> Result<i16>;
    /// Read an 8-bit signed int
    async fn read_i8_async(&mut self) -> Result<i8>;

    /// Read a 64-bit floating point int
    async fn read_f64_async(&mut self) -> Result<f64>;
    /// Read a 32-bit floating point int
    async fn read_f32_async(&mut self) -> Result<f32>;

    /// Read a boolean
    async fn read_bool_async(&mut self) -> Result<bool>;

    /// Read a byte slice
    async fn read_slice_async(&mut self, slice: &mut [u8]) -> Result<()>;
}

#[async_trait]
impl<W: AsyncWrite + Unpin + Send> AsyncWriteExt for W {
    #[inline]
    async fn write_u128_async(&mut self, v: u128) -> Result<()> {
        self.write_all(&endian::u128_to_array_le(v)).await
    }

    #[inline]
    async fn write_u64_async(&mut self, v: u64) -> Result<()> {
        self.write_all(&endian::u64_to_array_le(v)).await
    }

    #[inline]
    async fn write_u32_async(&mut self, v: u32) -> Result<()> {
        self.write_all(&endian::u32_to_array_le(v)).await
    }

    #[inline]
    async fn write_u16_async(&mut self, v: u16) -> Result<()> {
        self.write_all(&endian::u16_to_array_le(v)).await
    }

    #[inline]
    async fn write_u8_async(&mut self, v: u8) -> Result<()> {
        self.write_all(&[v]).await
    }

    #[inline]
    async fn write_i128_async(&mut self, v: i128) -> Result<()> {
        self.write_all(&endian::i128_to_array_le(v)).await
    }

    #[inline]
    async fn write_i64_async(&mut self, v: i64) -> Result<()> {
        self.write_all(&endian::i64_to_array_le(v)).await
    }

    #[inline]
    async fn write_i32_async(&mut self, v: i32) -> Result<()> {
        self.write_all(&endian::i32_to_array_le(v)).await
    }

    #[inline]
    async fn write_i16_async(&mut self, v: i16) -> Result<()> {
        self.write_all(&endian::i16_to_array_le(v)).await
    }

    #[inline]
    async fn write_i8_async(&mut self, v: i8) -> Result<()> {
        self.write_all(&[v as u8]).await
    }

    #[inline]
    async fn write_f64_async(&mut self, v: f64) -> Result<()> {
        self.write_all(&endian::f64_to_array_le(v)).await
    }

    #[inline]
    async fn write_f32_async(&mut self, v: f32) -> Result<()> {
        self.write_all(&endian::f32_to_array_le(v)).await
    }

    #[inline]
    async fn write_bool_async(&mut self, v: bool) -> Result<()> {
        self.write_all(&[v as u8]).await
    }

    #[inline]
    async fn write_slice_async(&mut self, v: &[u8]) -> Result<()> {
        self.write_all(v).await
    }
}

#[async_trait]
impl<R: AsyncRead + Unpin + Send> AsyncReadExt for R {
    #[inline]
    async fn read_u128_async(&mut self) -> Result<u128> {
        let mut val = [0; 16];
        self.read_exact(&mut val[..]).await?;
        Ok(endian::slice_to_u128_le(&val))
    }

    #[inline]
    async fn read_u64_async(&mut self) -> Result<u64> {
        let mut val = [0; 8];
        self.read_exact(&mut val[..]).await?;
        Ok(endian::slice_to_u64_le(&val))
    }

    #[inline]
    async fn read_u32_async(&mut self) -> Result<u32> {
        let mut val = [0; 4];
        self.read_exact(&mut val[..]).await?;
        Ok(endian::slice_to_u32_le(&val))
    }

    #[inline]
    async fn read_u16_async(&mut self) -> Result<u16> {
        let mut val = [0; 2];
        self.read_exact(&mut val[..]).await?;
        Ok(endian::slice_to_u16_le(&val))
    }

    #[inline]
    async fn read_u8_async(&mut self) -> Result<u8> {
        let mut val = [0; 1];
        self.read_exact(&mut val[..]).await?;
        Ok(val[0])
    }

    #[inline]
    async fn read_i128_async(&mut self) -> Result<i128> {
        let mut val = [0; 16];
        self.read_exact(&mut val[..]).await?;
        Ok(endian::slice_to_i128_le(&val))
    }

    #[inline]
    async fn read_i64_async(&mut self) -> Result<i64> {
        let mut val = [0; 8];
        self.read_exact(&mut val[..]).await?;
        Ok(endian::slice_to_i64_le(&val))
    }

    #[inline]
    async fn read_i32_async(&mut self) -> Result<i32> {
        let mut val = [0; 4];
        self.read_exact(&mut val[..]).await?;
        Ok(endian::slice_to_i32_le(&val))
    }

    #[inline]
    async fn read_i16_async(&mut self) -> Result<i16> {
        let mut val = [0; 2];
        self.read_exact(&mut val[..]).await?;
        Ok(endian::slice_to_i16_le(&val))
    }

    #[inline]
    async fn read_i8_async(&mut self) -> Result<i8> {
        let mut val = [0; 1];
        self.read_exact(&mut val[..]).await?;
        Ok(val[0] as i8)
    }

    #[inline]
    async fn read_f64_async(&mut self) -> Result<f64> {
        let mut val = [0; 8];
        self.read_exact(&mut val[..]).await?;
        Ok(endian::slice_to_f64_le(&val))
    }

    #[inline]
    async fn read_f32_async(&mut self) -> Result<f32> {
        let mut val = [0; 4];
        self.read_exact(&mut val[..]).await?;
        Ok(endian::slice_to_f32_le(&val))
    }

    #[inline]
    async fn read_bool_async(&mut self) -> Result<bool> {
        AsyncReadExt::read_i8_async(self).await.map(|bit| bit != 0)
    }

    #[inline]
    async fn read_slice_async(&mut self, slice: &mut [u8]) -> Result<()> {
        self.read_exact(slice).await
    }
}

macro_rules! impl_int_encodable {
    ($ty:ident, $meth_dec:ident, $meth_enc:ident) => {
        #[async_trait]
        impl AsyncDecodable for $ty {
            #[inline]
            async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
                AsyncReadExt::$meth_dec(d).await.map($ty::from_le)
            }
        }

        #[async_trait]
        impl AsyncEncodable for $ty {
            #[inline]
            async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
                s.$meth_enc(self.to_le()).await?;
                Ok(core::mem::size_of::<$ty>())
            }
        }
    };
}

impl_int_encodable!(u8, read_u8_async, write_u8_async);
impl_int_encodable!(u16, read_u16_async, write_u16_async);
impl_int_encodable!(u32, read_u32_async, write_u32_async);
impl_int_encodable!(u64, read_u64_async, write_u64_async);
impl_int_encodable!(u128, read_u128_async, write_u128_async);

impl_int_encodable!(i8, read_i8_async, write_i8_async);
impl_int_encodable!(i16, read_i16_async, write_i16_async);
impl_int_encodable!(i32, read_i32_async, write_i32_async);
impl_int_encodable!(i64, read_i64_async, write_i64_async);
impl_int_encodable!(i128, read_i128_async, write_i128_async);

macro_rules! tuple_encode {
    ($($x:ident),*) => (
        #[async_trait]
        impl<$($x: AsyncEncodable + Sync),*> AsyncEncodable for ($($x),*) {
            #[inline]
            #[allow(non_snake_case)]
            async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
                let &($(ref $x),*) = self;
                let mut len = 0;
                $(len += $x.encode_async(s).await?;)*
                Ok(len)
            }
        }

        #[async_trait]
        impl<$($x: AsyncDecodable + Send),*> AsyncDecodable for ($($x),*) {
            #[inline]
            #[allow(non_snake_case)]
            async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
                Ok(($({let $x = AsyncDecodable::decode_async(d).await?; $x }),*))
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

/// Asynchronously encode a dynamic set of arguments to a buffer.
#[macro_export]
macro_rules! encode_payload_async {
    ($buf:expr, $($args:expr),*) => {{ $( $args.encode_async($buf).await?;)* }}
}

#[async_trait]
impl AsyncEncodable for VarInt {
    #[inline]
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        match self.0 {
            0..=0xFC => {
                (self.0 as u8).encode_async(s).await?;
                Ok(1)
            }

            0xFD..=0xFFFF => {
                s.write_u8_async(0xFD).await?;
                (self.0 as u16).encode_async(s).await?;
                Ok(3)
            }

            0x10000..=0xFFFFFFFF => {
                s.write_u8_async(0xFE).await?;
                (self.0 as u32).encode_async(s).await?;
                Ok(5)
            }

            _ => {
                s.write_u8_async(0xFF).await?;
                self.0.encode_async(s).await?;
                Ok(9)
            }
        }
    }
}

#[async_trait]
impl AsyncDecodable for VarInt {
    #[inline]
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let n = AsyncReadExt::read_u8_async(d).await?;
        match n {
            0xFF => {
                let x = AsyncReadExt::read_u64_async(d).await?;
                if x < 0x100000000 {
                    return Err(Error::new(ErrorKind::Other, "Non-minimal VarInt"))
                }
                Ok(VarInt(x))
            }

            0xFE => {
                let x = AsyncReadExt::read_u32_async(d).await?;
                if x < 0x10000 {
                    return Err(Error::new(ErrorKind::Other, "Non-minimal VarInt"))
                }
                Ok(VarInt(x as u64))
            }

            0xFD => {
                let x = AsyncReadExt::read_u16_async(d).await?;
                if x < 0xFD {
                    return Err(Error::new(ErrorKind::Other, "Non-minimal VarInt"))
                }
                Ok(VarInt(x as u64))
            }

            n => Ok(VarInt(n as u64)),
        }
    }
}

// Implementations for some primitive types
#[async_trait]
impl AsyncEncodable for usize {
    #[inline]
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        s.write_u64_async(*self as u64).await?;
        Ok(8)
    }
}

#[async_trait]
impl AsyncDecodable for usize {
    #[inline]
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        Ok(AsyncReadExt::read_u64_async(d).await? as usize)
    }
}

#[async_trait]
impl AsyncEncodable for f64 {
    #[inline]
    async fn encode_async<S: AsyncWriteExt + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        s.write_f64_async(*self).await?;
        Ok(core::mem::size_of::<f64>())
    }
}

#[async_trait]
impl AsyncDecodable for f64 {
    #[inline]
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        AsyncReadExt::read_f64_async(d).await
    }
}

#[async_trait]
impl AsyncEncodable for f32 {
    #[inline]
    async fn encode_async<S: AsyncWriteExt + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        s.write_f32_async(*self).await?;
        Ok(core::mem::size_of::<f32>())
    }
}

#[async_trait]
impl AsyncDecodable for f32 {
    #[inline]
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        AsyncReadExt::read_f32_async(d).await
    }
}

#[async_trait]
impl AsyncEncodable for bool {
    #[inline]
    async fn encode_async<S: AsyncWriteExt + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        s.write_bool_async(*self).await?;
        Ok(1)
    }
}

#[async_trait]
impl AsyncDecodable for bool {
    #[inline]
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        AsyncReadExt::read_bool_async(d).await
    }
}

#[async_trait]
impl<T: AsyncEncodable + Sync> AsyncEncodable for Vec<T> {
    #[inline]
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += VarInt(self.len() as u64).encode_async(s).await?;
        for val in self {
            len += val.encode_async(s).await?;
        }
        Ok(len)
    }
}

#[async_trait]
impl<T: AsyncDecodable + Send> AsyncDecodable for Vec<T> {
    #[inline]
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let len = VarInt::decode_async(d).await?.0;
        let mut ret = Vec::new();
        ret.try_reserve(len as usize).map_err(|_| std::io::ErrorKind::InvalidData)?;
        for _ in 0..len {
            ret.push(AsyncDecodable::decode_async(d).await?);
        }
        Ok(ret)
    }
}

#[async_trait]
impl<T: AsyncEncodable + Sync> AsyncEncodable for VecDeque<T> {
    #[inline]
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += VarInt(self.len() as u64).encode_async(s).await?;
        for val in self {
            len += val.encode_async(s).await?;
        }
        Ok(len)
    }
}

#[async_trait]
impl<T: AsyncDecodable + Send> AsyncDecodable for VecDeque<T> {
    #[inline]
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let len = VarInt::decode_async(d).await?.0;
        let mut ret = VecDeque::new();
        ret.try_reserve(len as usize).map_err(|_| std::io::ErrorKind::InvalidData)?;
        for _ in 0..len {
            ret.push_back(AsyncDecodable::decode_async(d).await?);
        }
        Ok(ret)
    }
}

#[async_trait]
impl<T: AsyncEncodable + Sync> AsyncEncodable for Option<T> {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        if let Some(v) = self {
            len += true.encode_async(s).await?;
            len += v.encode_async(s).await?;
        } else {
            len += false.encode_async(s).await?;
        }
        Ok(len)
    }
}

#[async_trait]
impl<T: AsyncDecodable> AsyncDecodable for Option<T> {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let valid: bool = AsyncDecodable::decode_async(d).await?;
        let val = if valid { Some(AsyncDecodable::decode_async(d).await?) } else { None };
        Ok(val)
    }
}

#[async_trait]
impl<T, const N: usize> AsyncEncodable for [T; N]
where
    T: AsyncEncodable + Sync,
{
    #[inline]
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        for elem in self.iter() {
            len += elem.encode_async(s).await?;
        }

        Ok(len)
    }
}

#[async_trait]
impl<T, const N: usize> AsyncDecodable for [T; N]
where
    T: AsyncDecodable + Send + core::fmt::Debug,
{
    #[inline]
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let mut ret = vec![];
        for _ in 0..N {
            ret.push(AsyncDecodable::decode_async(d).await?);
        }

        Ok(ret.try_into().unwrap())
    }
}

#[async_trait]
impl AsyncEncodable for String {
    #[inline]
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let b = self.as_bytes();
        let b_len = b.len();
        let vi_len = VarInt(b_len as u64).encode_async(s).await?;
        s.write_slice_async(b).await?;
        Ok(vi_len + b_len)
    }
}

#[async_trait]
impl AsyncEncodable for &str {
    #[inline]
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let b = self.as_bytes();
        let b_len = b.len();
        let vi_len = VarInt(b_len as u64).encode_async(s).await?;
        s.write_slice_async(b).await?;
        Ok(vi_len + b_len)
    }
}

#[async_trait]
impl AsyncDecodable for String {
    #[inline]
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<String> {
        match String::from_utf8(AsyncDecodable::decode_async(d).await?) {
            Ok(v) => Ok(v),
            Err(_) => Err(Error::new(ErrorKind::Other, "Invalid UTF-8 for string")),
        }
    }
}
