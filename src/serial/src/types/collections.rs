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

//! Serialization of collections
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    io::{Read, Result, Write},
};

#[cfg(feature = "async")]
use crate::{AsyncDecodable, AsyncEncodable};
#[cfg(feature = "async")]
use async_trait::async_trait;
#[cfg(feature = "async")]
use futures_lite::{AsyncRead, AsyncWrite};

use crate::{Decodable, Encodable, VarInt};

impl<T: Encodable> Encodable for HashSet<T> {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += VarInt(self.len() as u64).encode(s)?;
        for c in self.iter() {
            len += c.encode(s)?;
        }
        Ok(len)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl<T: AsyncEncodable + Sync> AsyncEncodable for HashSet<T> {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += VarInt(self.len() as u64).encode_async(s).await?;
        for c in self.iter() {
            len += c.encode_async(s).await?;
        }
        Ok(len)
    }
}

impl<T: Decodable + std::cmp::Eq + std::hash::Hash> Decodable for HashSet<T> {
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
        let len = VarInt::decode(d)?.0;
        let mut ret = HashSet::new();
        for _ in 0..len {
            let entry: T = Decodable::decode(d)?;
            ret.insert(entry);
        }
        Ok(ret)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl<T: AsyncDecodable + Send + std::cmp::Eq + std::hash::Hash> AsyncDecodable for HashSet<T> {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let len = VarInt::decode_async(d).await?.0;
        let mut ret = HashSet::new();
        for _ in 0..len {
            let entry: T = AsyncDecodable::decode_async(d).await?;
            ret.insert(entry);
        }
        Ok(ret)
    }
}

impl<T: Encodable, U: Encodable> Encodable for BTreeMap<T, U> {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += VarInt(self.len() as u64).encode(s)?;
        for c in self.iter() {
            len += c.0.encode(s)?;
            len += c.1.encode(s)?;
        }
        Ok(len)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl<T: AsyncEncodable + Sync, U: AsyncEncodable + Sync> AsyncEncodable for BTreeMap<T, U> {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += VarInt(self.len() as u64).encode_async(s).await?;
        for c in self.iter() {
            len += c.0.encode_async(s).await?;
            len += c.1.encode_async(s).await?;
        }
        Ok(len)
    }
}

impl<T: Decodable + std::cmp::Ord, U: Decodable> Decodable for BTreeMap<T, U> {
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
        let len = VarInt::decode(d)?.0;
        let mut ret = BTreeMap::new();
        for _ in 0..len {
            let key: T = Decodable::decode(d)?;
            let entry: U = Decodable::decode(d)?;
            ret.insert(key, entry);
        }
        Ok(ret)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl<T: AsyncDecodable + Send + std::cmp::Ord, U: AsyncDecodable + Send> AsyncDecodable
    for BTreeMap<T, U>
{
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let len = VarInt::decode_async(d).await?.0;
        let mut ret = BTreeMap::new();
        for _ in 0..len {
            let key: T = AsyncDecodable::decode_async(d).await?;
            let entry: U = AsyncDecodable::decode_async(d).await?;
            ret.insert(key, entry);
        }
        Ok(ret)
    }
}

impl<T: Encodable> Encodable for BTreeSet<T> {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += VarInt(self.len() as u64).encode(s)?;
        for c in self.iter() {
            len += c.encode(s)?;
        }
        Ok(len)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl<T: AsyncEncodable + Sync> AsyncEncodable for BTreeSet<T> {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += VarInt(self.len() as u64).encode_async(s).await?;
        for c in self.iter() {
            len += c.encode_async(s).await?;
        }
        Ok(len)
    }
}

impl<T: Decodable + std::cmp::Ord> Decodable for BTreeSet<T> {
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
        let len = VarInt::decode(d)?.0;
        let mut ret = BTreeSet::new();
        for _ in 0..len {
            let key: T = Decodable::decode(d)?;
            ret.insert(key);
        }
        Ok(ret)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl<T: AsyncDecodable + Send + std::cmp::Ord> AsyncDecodable for BTreeSet<T> {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let len = VarInt::decode_async(d).await?.0;
        let mut ret = BTreeSet::new();
        for _ in 0..len {
            let key: T = AsyncDecodable::decode_async(d).await?;
            ret.insert(key);
        }
        Ok(ret)
    }
}

impl<T: Encodable, U: Encodable> Encodable for HashMap<T, U> {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += VarInt(self.len() as u64).encode(s)?;
        for c in self.iter() {
            len += c.0.encode(s)?;
            len += c.1.encode(s)?;
        }
        Ok(len)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl<T: AsyncEncodable + Sync, U: AsyncEncodable + Sync> AsyncEncodable for HashMap<T, U> {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += VarInt(self.len() as u64).encode_async(s).await?;
        for c in self.iter() {
            len += c.0.encode_async(s).await?;
            len += c.1.encode_async(s).await?;
        }
        Ok(len)
    }
}

impl<T: Decodable + std::cmp::Eq + std::hash::Hash, U: Decodable> Decodable for HashMap<T, U> {
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
        let len = VarInt::decode(d)?.0;
        let mut ret = HashMap::new();
        for _ in 0..len {
            let key: T = Decodable::decode(d)?;
            let entry: U = Decodable::decode(d)?;
            ret.insert(key, entry);
        }
        Ok(ret)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl<T: AsyncDecodable + Send + std::cmp::Eq + std::hash::Hash, U: AsyncDecodable + Send>
    AsyncDecodable for HashMap<T, U>
{
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let len = VarInt::decode_async(d).await?.0;
        let mut ret = HashMap::new();
        for _ in 0..len {
            let key: T = AsyncDecodable::decode_async(d).await?;
            let entry: U = AsyncDecodable::decode_async(d).await?;
            ret.insert(key, entry);
        }
        Ok(ret)
    }
}
