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

use std::io::{Error, ErrorKind, Read, Result, Write};
use url::Url;

#[cfg(feature = "async")]
use crate::{AsyncDecodable, AsyncEncodable};
#[cfg(feature = "async")]
use async_trait::async_trait;
#[cfg(feature = "async")]
use futures_lite::{AsyncRead, AsyncWrite};

use crate::{Decodable, Encodable};

impl Encodable for Url {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
        self.as_str().encode(s)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncEncodable for Url {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        self.as_str().encode_async(s).await
    }
}

impl Decodable for Url {
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
        let s: String = Decodable::decode(d)?;
        match Url::parse(&s) {
            Ok(v) => Ok(v),
            Err(e) => Err(Error::new(ErrorKind::Other, e)),
        }
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncDecodable for Url {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let s: String = AsyncDecodable::decode_async(d).await?;
        match Url::parse(&s) {
            Ok(v) => Ok(v),
            Err(e) => Err(Error::new(ErrorKind::Other, e)),
        }
    }
}
