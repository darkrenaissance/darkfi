/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use std::io::{Read, Result, Write};

#[cfg(feature = "async")]
use crate::{AsyncDecodable, AsyncEncodable};
#[cfg(feature = "async")]
use async_trait::async_trait;
#[cfg(feature = "async")]
use futures_lite::{AsyncRead, AsyncWrite};

use crate::{Decodable, Encodable};

impl Encodable for num_bigint::BigInt {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
        self.to_signed_bytes_be().encode(s)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncEncodable for num_bigint::BigInt {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        self.to_signed_bytes_be().encode_async(s).await
    }
}

impl Decodable for num_bigint::BigInt {
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
        let vec: Vec<u8> = Decodable::decode(d)?;
        Ok(Self::from_signed_bytes_be(&vec))
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncDecodable for num_bigint::BigInt {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let vec: Vec<u8> = AsyncDecodable::decode_async(d).await?;
        Ok(Self::from_signed_bytes_be(&vec))
    }
}

impl Encodable for num_bigint::BigUint {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
        self.to_bytes_be().encode(s)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncEncodable for num_bigint::BigUint {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        self.to_bytes_be().encode_async(s).await
    }
}

impl Decodable for num_bigint::BigUint {
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
        let vec: Vec<u8> = Decodable::decode(d)?;
        Ok(Self::from_bytes_be(&vec))
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncDecodable for num_bigint::BigUint {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let vec: Vec<u8> = AsyncDecodable::decode_async(d).await?;
        Ok(Self::from_bytes_be(&vec))
    }
}
