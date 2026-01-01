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

use std::io::{Error, ErrorKind, Read, Result, Write};

#[cfg(feature = "async")]
use crate::{AsyncDecodable, AsyncEncodable};
#[cfg(feature = "async")]
use async_trait::async_trait;
#[cfg(feature = "async")]
use futures_lite::{AsyncRead, AsyncWrite};

use crate::{Decodable, Encodable};

impl Encodable for semver::Prerelease {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
        self.as_str().encode(s)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncEncodable for semver::Prerelease {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        self.as_str().encode_async(s).await
    }
}

impl Decodable for semver::Prerelease {
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
        let s: String = Decodable::decode(d)?;

        match Self::new(&s) {
            Ok(v) => Ok(v),
            Err(_) => Err(Error::new(ErrorKind::Other, "Failed deserializing semver::Prerelase")),
        }
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncDecodable for semver::Prerelease {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let s: String = AsyncDecodable::decode_async(d).await?;

        match Self::new(&s) {
            Ok(v) => Ok(v),
            Err(_) => Err(Error::new(ErrorKind::Other, "Failed deserializing semver::Prerelease")),
        }
    }
}

impl Encodable for semver::BuildMetadata {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
        self.as_str().encode(s)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncEncodable for semver::BuildMetadata {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        self.as_str().encode_async(s).await
    }
}

impl Decodable for semver::BuildMetadata {
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
        let s: String = Decodable::decode(d)?;

        match Self::new(&s) {
            Ok(v) => Ok(v),
            Err(_) => {
                Err(Error::new(ErrorKind::Other, "Failed deserializing semver::BuildMetadata"))
            }
        }
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncDecodable for semver::BuildMetadata {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let s: String = AsyncDecodable::decode_async(d).await?;

        match Self::new(&s) {
            Ok(v) => Ok(v),
            Err(_) => {
                Err(Error::new(ErrorKind::Other, "Failed deserializing semver::BuildMetadata"))
            }
        }
    }
}

impl Encodable for semver::Version {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += self.major.encode(s)?;
        len += self.minor.encode(s)?;
        len += self.patch.encode(s)?;
        len += self.pre.encode(s)?;
        len += self.build.encode(s)?;
        Ok(len)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncEncodable for semver::Version {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += self.major.encode_async(s).await?;
        len += self.minor.encode_async(s).await?;
        len += self.patch.encode_async(s).await?;
        len += self.pre.encode_async(s).await?;
        len += self.build.encode_async(s).await?;
        Ok(len)
    }
}

impl Decodable for semver::Version {
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
        let major: u64 = Decodable::decode(d)?;
        let minor: u64 = Decodable::decode(d)?;
        let patch: u64 = Decodable::decode(d)?;
        let pre: semver::Prerelease = Decodable::decode(d)?;
        let build: semver::BuildMetadata = Decodable::decode(d)?;
        Ok(Self { major, minor, patch, pre, build })
    }
}
#[cfg(feature = "async")]
#[async_trait]
impl AsyncDecodable for semver::Version {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let major: u64 = AsyncDecodable::decode_async(d).await?;
        let minor: u64 = AsyncDecodable::decode_async(d).await?;
        let patch: u64 = AsyncDecodable::decode_async(d).await?;
        let pre: semver::Prerelease = AsyncDecodable::decode_async(d).await?;
        let build: semver::BuildMetadata = AsyncDecodable::decode_async(d).await?;
        Ok(Self { major, minor, patch, pre, build })
    }
}

#[cfg(test)]
mod tests {
    use crate::{deserialize, serialize};

    #[test]
    fn serialize_deserialize_semver() {
        let versions = vec!["1.12.0", "1.21.3-beta", "2.0.0-rc.1"];

        for version in versions {
            let original = semver::Version::parse(version).unwrap();
            let serialized = serialize(&original);
            let deserialized = deserialize(&serialized).unwrap();
            assert_eq!(original, deserialized);
        }
    }
}
