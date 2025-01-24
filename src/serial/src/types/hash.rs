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
use crate::{
    async_lib::{AsyncReadExt, AsyncWriteExt},
    async_trait, AsyncDecodable, AsyncEncodable, AsyncRead, AsyncWrite,
};

use crate::{Decodable, Encodable, ReadExt, WriteExt};

#[cfg(feature = "blake3")]
impl Encodable for blake3::Hash {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
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
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
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
