/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use std::io::{Error, ErrorKind};

use futures_lite::{
    AsyncRead, AsyncReadExt as AsyncReadExtFut, AsyncWrite, AsyncWriteExt as AsyncWriteExtFut,
};

use super::{endian, VarInt};

pub struct AsyncReadExt;
pub struct AsyncWriteExt;

macro_rules! async_decoder_fn {
    ($name:ident, $val_type:ty, $readfn:ident, $byte_len:expr) => {
        #[inline]
        pub async fn $name<R: AsyncRead + Unpin>(stream: &mut R) -> Result<$val_type, Error> {
            assert_eq!(core::mem::size_of::<$val_type>(), $byte_len);
            let mut val = [0; $byte_len];
            stream.read_exact(&mut val[..]).await?;
            Ok(endian::$readfn(&val))
        }
    };
}

macro_rules! async_encoder_fn {
    ($name:ident, $val_type:ty, $writefn:ident) => {
        #[inline]
        pub async fn $name<W: AsyncWrite + Unpin>(
            stream: &mut W,
            v: $val_type,
        ) -> Result<(), Error> {
            stream.write_all(&endian::$writefn(v)).await
        }
    };
}

#[allow(dead_code)]
impl AsyncReadExt {
    async_decoder_fn!(read_u128, u128, slice_to_u128_le, 16);
    async_decoder_fn!(read_u64, u64, slice_to_u64_le, 8);
    async_decoder_fn!(read_u32, u32, slice_to_u32_le, 4);
    async_decoder_fn!(read_u16, u16, slice_to_u16_le, 2);

    pub async fn read_u8<R: AsyncRead + Unpin>(stream: &mut R) -> Result<u8, Error> {
        let mut slice = [0u8; 1];
        stream.read_exact(&mut slice).await?;
        Ok(slice[0])
    }
}

#[allow(dead_code)]
impl AsyncWriteExt {
    async_encoder_fn!(write_u128, u128, u128_to_array_le);
    async_encoder_fn!(write_u64, u64, u64_to_array_le);
    async_encoder_fn!(write_u32, u32, u32_to_array_le);
    async_encoder_fn!(write_u16, u16, u16_to_array_le);

    pub async fn write_u8<W: AsyncWrite + Unpin>(stream: &mut W, v: u8) -> Result<(), Error> {
        stream.write_all(&[v]).await
    }
}

impl VarInt {
    #[inline]
    pub async fn encode_async<W: AsyncWrite + Unpin>(
        &self,
        stream: &mut W,
    ) -> Result<usize, Error> {
        match self.0 {
            0..=0xFC => {
                AsyncWriteExt::write_u8(stream, self.0 as u8).await?;
                Ok(1)
            }

            0xFD..=0xFFFF => {
                AsyncWriteExt::write_u8(stream, 0xFD).await?;
                AsyncWriteExt::write_u16(stream, self.0 as u16).await?;
                Ok(3)
            }

            0x10000..=0xFFFFFFFF => {
                AsyncWriteExt::write_u8(stream, 0xFE).await?;
                AsyncWriteExt::write_u32(stream, self.0 as u32).await?;
                Ok(5)
            }

            _ => {
                AsyncWriteExt::write_u8(stream, 0xFF).await?;
                AsyncWriteExt::write_u64(stream, self.0).await?;
                Ok(9)
            }
        }
    }

    #[inline]
    pub async fn decode_async<R: AsyncRead + Unpin>(stream: &mut R) -> Result<Self, Error> {
        let n = AsyncReadExt::read_u8(stream).await?;
        match n {
            0xFF => {
                let x = AsyncReadExt::read_u64(stream).await?;
                if x < 0x100000000 {
                    return Err(Error::new(ErrorKind::Other, "Non-minimal VarInt"))
                }
                Ok(VarInt(x))
            }

            0xFE => {
                let x = AsyncReadExt::read_u32(stream).await?;
                if x < 0x10000 {
                    return Err(Error::new(ErrorKind::Other, "Non-minimal VarInt"))
                }
                Ok(VarInt(x as u64))
            }

            0xFD => {
                let x = AsyncReadExt::read_u16(stream).await?;
                if x < 0xFD {
                    return Err(Error::new(ErrorKind::Other, "Non-minimal VarInt"))
                }
                Ok(VarInt(x as u64))
            }

            n => Ok(VarInt(n as u64)),
        }
    }
}
