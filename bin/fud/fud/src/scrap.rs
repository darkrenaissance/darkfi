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

use darkfi_serial::{async_trait, AsyncDecodable, AsyncEncodable, AsyncRead, AsyncWrite};
use std::io::Result;

#[derive(Clone, Debug)]
pub struct Scrap {
    pub chunk: Vec<u8>,
    /// Hash of the data that was last written to the file system (parts of `chunk`).
    /// Used to check if the data on the filesystem changed and the scrap should be rewritten.
    pub hash_written: blake3::Hash,
}

#[async_trait]
impl AsyncEncodable for Scrap {
    #[inline]
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += self.chunk.encode_async(s).await?;
        len += self.hash_written.encode_async(s).await?;
        Ok(len)
    }
}

#[async_trait]
impl AsyncDecodable for Scrap {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        Ok(Self {
            chunk: <Vec<u8>>::decode_async(d).await?,
            hash_written: blake3::Hash::decode_async(d).await?,
        })
    }
}
