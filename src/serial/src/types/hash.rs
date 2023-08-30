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

use std::io::{Error, ErrorKind, Read, Result, Write};

#[cfg(feature = "async")]
use crate::{
    async_lib::{AsyncReadExt, AsyncWriteExt},
    async_trait, AsyncDecodable, AsyncEncodable, AsyncRead, AsyncWrite,
};

use crate::{Decodable, Encodable, ReadExt, WriteExt};

#[cfg(feature = "blake2b_simd")]
impl Encodable for blake2b_simd::Hash {
    fn encode<S: Write>(&self, mut s: S) -> Result<usize> {
        // The hash can be of variable output length.
        // We'll support 16, 32, and 64 bytes, otherwise panic.
        // This means we need 1 byte to tell the length.
        let len = self.as_bytes().len();
        if len != 16 && len != 32 && len != 64 {
            panic!("blake2b serialization supports only 16, 32, or 64 bytes");
        }

        s.write_u8(len as u8)?;
        s.write_slice(self.as_bytes())?;
        Ok(len + 1)
    }
}

#[cfg(all(feature = "blake2b_simd", feature = "async"))]
#[async_trait]
impl AsyncEncodable for blake2b_simd::Hash {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        // The hash can be of variable output length.
        // We'll support 16, 32 and 64 bytes, otherwise panic.
        // This means we need 1 byte to tell the length.
        let len = self.as_bytes().len();
        if len != 16 && len != 32 && len != 64 {
            panic!("blake2b serialization supports only 16, 32, or 64 bytes");
        }

        s.write_u8_async(len as u8).await?;
        s.write_slice_async(self.as_bytes()).await?;
        Ok(len)
    }
}

#[cfg(feature = "blake2b_simd")]
impl Decodable for blake2b_simd::Hash {
    fn decode<D: Read>(mut d: D) -> Result<Self> {
        let len = d.read_u8()?;

        if len == 16 {
            let mut bytes = [0u8; 16];
            d.read_slice(&mut bytes)?;
            Ok(blake2b_simd::Hash::from(bytes))
        } else if len == 32 {
            let mut bytes = [0u8; 32];
            d.read_slice(&mut bytes)?;
            Ok(blake2b_simd::Hash::from(bytes))
        } else if len == 64 {
            let mut bytes = [0u8; 64];
            d.read_slice(&mut bytes)?;
            Ok(blake2b_simd::Hash::from(bytes))
        } else {
            Err(Error::new(ErrorKind::Other, "Unsupported blake2b hash length"))
        }
    }
}

#[cfg(all(feature = "blake2b_simd", feature = "async"))]
#[async_trait]
impl AsyncDecodable for blake2b_simd::Hash {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let len = d.read_u8_async().await?;

        if len == 16 {
            let mut bytes = [0u8; 16];
            d.read_slice_async(&mut bytes).await?;
            Ok(blake2b_simd::Hash::from(bytes))
        } else if len == 32 {
            let mut bytes = [0u8; 32];
            d.read_slice_async(&mut bytes).await?;
            Ok(blake2b_simd::Hash::from(bytes))
        } else if len == 64 {
            let mut bytes = [0u8; 64];
            d.read_slice_async(&mut bytes).await?;
            Ok(blake2b_simd::Hash::from(bytes))
        } else {
            Err(Error::new(ErrorKind::Other, "Unsupported blake2b hash length"))
        }
    }
}

#[cfg(feature = "blake3")]
impl Encodable for blake3::Hash {
    fn encode<S: Write>(&self, mut s: S) -> Result<usize> {
        s.write_slice(self.as_bytes())?;
        Ok(blake3::OUT_LEN)
    }
}

#[cfg(all(feature = "blake3", feature = "async"))]
#[async_trait]
impl AsyncEncodable for blake3::Hash {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        s.write_slice_async(self.as_bytes()).await?;
        Ok(blake3::OUT_LEN)
    }
}

#[cfg(feature = "blake3")]
impl Decodable for blake3::Hash {
    fn decode<D: Read>(mut d: D) -> Result<Self> {
        let mut bytes = [0u8; blake3::OUT_LEN];
        d.read_slice(&mut bytes)?;
        Ok(bytes.into())
    }
}

#[cfg(all(feature = "blake3", feature = "async"))]
#[async_trait]
impl AsyncDecodable for blake3::Hash {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let mut bytes = [0u8; blake3::OUT_LEN];
        d.read_slice_async(&mut bytes).await?;
        Ok(bytes.into())
    }
}

#[cfg(test)]
mod tests {
    use crate::{deserialize, serialize};

    #[test]
    fn serialize_deserialize_blake2b() {
        let hash16 =
            blake2b_simd::Params::new().hash_length(16).to_state().update(b"foo").finalize();
        let hash16_ser = serialize(&hash16);
        assert!(hash16_ser.len() == 17);

        let hash16_de: blake2b_simd::Hash = deserialize(&hash16_ser).unwrap();
        assert!(hash16 == hash16_de);

        let hash32 =
            blake2b_simd::Params::new().hash_length(32).to_state().update(b"foo").finalize();
        let hash32_ser = serialize(&hash32);
        assert!(hash32_ser.len() == 33);

        let hash32_de: blake2b_simd::Hash = deserialize(&hash32_ser).unwrap();
        assert!(hash32 == hash32_de);

        let hash64 =
            blake2b_simd::Params::new().hash_length(64).to_state().update(b"foo").finalize();
        let hash64_ser = serialize(&hash64);
        assert!(hash64_ser.len() == 65);

        let hash64_de: blake2b_simd::Hash = deserialize(&hash64_ser).unwrap();
        assert!(hash64 == hash64_de);
    }
}
