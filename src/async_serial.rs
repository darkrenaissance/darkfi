use futures::prelude::*;

use crate::endian;
use crate::error::{Error, Result};
use crate::serial::VarInt;

impl VarInt {
    pub async fn encode_async<W: AsyncWrite + Unpin>(&self, stream: &mut W) -> Result<usize> {
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
                AsyncWriteExt::write_u64(stream, self.0 as u64).await?;
                Ok(9)
            }
        }
    }

    pub async fn decode_async<R: AsyncRead + Unpin>(stream: &mut R) -> Result<Self> {
        let n = AsyncReadExt::read_u8(stream).await?;
        match n {
            0xFF => {
                let x = AsyncReadExt::read_u64(stream).await?;
                if x < 0x100000000 {
                    Err(Error::NonMinimalVarInt)
                } else {
                    Ok(VarInt(x))
                }
            }
            0xFE => {
                let x = AsyncReadExt::read_u32(stream).await?;
                if x < 0x10000 {
                    Err(Error::NonMinimalVarInt)
                } else {
                    Ok(VarInt(x as u64))
                }
            }
            0xFD => {
                let x = AsyncReadExt::read_u16(stream).await?;
                if x < 0xFD {
                    Err(Error::NonMinimalVarInt)
                } else {
                    Ok(VarInt(x as u64))
                }
            }
            n => Ok(VarInt(n as u64)),
        }
    }
}

macro_rules! async_encoder_fn {
    ($name:ident, $val_type:ty, $writefn:ident) => {
        #[inline]
        pub async fn $name<W: AsyncWrite + Unpin>(stream: &mut W, v: $val_type) -> Result<()> {
            stream
                .write_all(&endian::$writefn(v))
                .await
                .map_err(|e| Error::Io(e.kind()))
        }
    };
}

macro_rules! async_decoder_fn {
    ($name:ident, $val_type:ty, $readfn:ident, $byte_len: expr) => {
        pub async fn $name<R: AsyncRead + Unpin>(stream: &mut R) -> Result<$val_type> {
            assert_eq!(::std::mem::size_of::<$val_type>(), $byte_len); // size_of isn't a constfn in 1.22
            let mut val = [0; $byte_len];
            stream
                .read_exact(&mut val[..])
                .await
                .map_err(|e| Error::Io(e.kind()))?;
            Ok(endian::$readfn(&val))
        }
    };
}

pub struct AsyncReadExt {}

impl AsyncReadExt {
    async_decoder_fn!(read_u64, u64, slice_to_u64_le, 8);
    async_decoder_fn!(read_u32, u32, slice_to_u32_le, 4);
    async_decoder_fn!(read_u16, u16, slice_to_u16_le, 2);

    pub async fn read_u8<R: AsyncRead + Unpin>(stream: &mut R) -> Result<u8> {
        let mut slice = [0u8; 1];
        stream.read_exact(&mut slice).await?;
        Ok(slice[0])
    }
}

pub struct AsyncWriteExt {}

impl AsyncWriteExt {
    async_encoder_fn!(write_u64, u64, u64_to_array_le);
    async_encoder_fn!(write_u32, u32, u32_to_array_le);
    async_encoder_fn!(write_u16, u16, u16_to_array_le);

    pub async fn write_u8<W: AsyncWrite + Unpin>(stream: &mut W, v: u8) -> Result<()> {
        stream
            .write_all(&[v])
            .await
            .map_err(|e| Error::Io(e.kind()))
    }
}
