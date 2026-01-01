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

use core::fmt::Debug;
use std::io::{Error, Read, Result, Write};

#[cfg(feature = "async")]
use crate::{AsyncDecodable, AsyncEncodable};
#[cfg(feature = "async")]
use async_trait::async_trait;
#[cfg(feature = "async")]
use futures_lite::{AsyncRead, AsyncWrite};

use crate::{Decodable, Encodable};

impl Encodable for bridgetree::Position {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
        u64::from(*self).encode(s)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncEncodable for bridgetree::Position {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        u64::from(*self).encode_async(s).await
    }
}

impl Decodable for bridgetree::Position {
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
        let dec: u64 = Decodable::decode(d)?;
        Ok(Self::from(dec))
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncDecodable for bridgetree::Position {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let dec: u64 = AsyncDecodable::decode_async(d).await?;
        Ok(Self::from(dec))
    }
}

impl Encodable for bridgetree::Address {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += u8::from(self.level()).encode(s)?;
        len += self.index().encode(s)?;
        Ok(len)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncEncodable for bridgetree::Address {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += u8::from(self.level()).encode_async(s).await?;
        len += self.index().encode_async(s).await?;
        Ok(len)
    }
}

impl Decodable for bridgetree::Address {
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
        let level: u8 = Decodable::decode(d)?;
        let index = Decodable::decode(d)?;
        Ok(Self::from_parts(level.into(), index))
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncDecodable for bridgetree::Address {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let level: u8 = AsyncDecodable::decode_async(d).await?;
        let index = AsyncDecodable::decode_async(d).await?;
        Ok(Self::from_parts(level.into(), index))
    }
}

impl<H: Encodable + Ord + Clone> Encodable for bridgetree::NonEmptyFrontier<H> {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += self.position().encode(s)?;
        len += self.leaf().encode(s)?;
        len += self.ommers().to_vec().encode(s)?;
        Ok(len)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl<H: AsyncEncodable + Sync + Send + Ord + Clone> AsyncEncodable
    for bridgetree::NonEmptyFrontier<H>
{
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += self.position().encode_async(s).await?;
        len += self.leaf().encode_async(s).await?;
        len += self.ommers().to_vec().encode_async(s).await?;
        Ok(len)
    }
}

impl<H: Decodable + Ord + Clone> Decodable for bridgetree::NonEmptyFrontier<H> {
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
        let position = Decodable::decode(d)?;
        let leaf = Decodable::decode(d)?;
        let ommers = Decodable::decode(d)?;

        match Self::from_parts(position, leaf, ommers) {
            Ok(v) => Ok(v),
            Err(_) => Err(Error::other("FrontierError")),
        }
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl<H: AsyncDecodable + Send + Ord + Clone> AsyncDecodable for bridgetree::NonEmptyFrontier<H> {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let position = AsyncDecodable::decode_async(d).await?;
        let leaf = AsyncDecodable::decode_async(d).await?;
        let ommers = AsyncDecodable::decode_async(d).await?;

        match Self::from_parts(position, leaf, ommers) {
            Ok(v) => Ok(v),
            Err(_) => Err(Error::other("FrontierError")),
        }
    }
}

impl<H: Encodable + Ord + Clone> Encodable for bridgetree::MerkleBridge<H> {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += self.prior_position().encode(s)?;
        len += self.tracking().encode(s)?;
        len += self.ommers().encode(s)?;
        len += self.frontier().encode(s)?;
        Ok(len)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl<H: AsyncEncodable + Sync + Send + Ord + Clone> AsyncEncodable for bridgetree::MerkleBridge<H> {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += self.prior_position().encode_async(s).await?;
        len += self.tracking().encode_async(s).await?;
        len += self.ommers().encode_async(s).await?;
        len += self.frontier().encode_async(s).await?;
        Ok(len)
    }
}

impl<H: Decodable + Ord + Clone> Decodable for bridgetree::MerkleBridge<H> {
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
        let prior_position = Decodable::decode(d)?;
        let tracking = Decodable::decode(d)?;
        let ommers = Decodable::decode(d)?;
        let frontier = Decodable::decode(d)?;
        Ok(Self::from_parts(prior_position, tracking, ommers, frontier))
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl<H: AsyncDecodable + Send + Ord + Clone> AsyncDecodable for bridgetree::MerkleBridge<H> {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let prior_position = AsyncDecodable::decode_async(d).await?;
        let tracking = AsyncDecodable::decode_async(d).await?;
        let ommers = AsyncDecodable::decode_async(d).await?;
        let frontier = AsyncDecodable::decode_async(d).await?;
        Ok(Self::from_parts(prior_position, tracking, ommers, frontier))
    }
}

impl<C: Encodable> Encodable for bridgetree::Checkpoint<C> {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += self.id().encode(s)?;
        len += self.bridges_len().encode(s)?;
        len += self.marked().encode(s)?;
        len += self.forgotten().encode(s)?;
        Ok(len)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl<C: AsyncEncodable + Sync> AsyncEncodable for bridgetree::Checkpoint<C> {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += self.id().encode_async(s).await?;
        len += self.bridges_len().encode_async(s).await?;
        len += self.marked().encode_async(s).await?;
        len += self.forgotten().encode_async(s).await?;
        Ok(len)
    }
}

impl<C: Decodable> Decodable for bridgetree::Checkpoint<C> {
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
        let id = Decodable::decode(d)?;
        let bridges_len = Decodable::decode(d)?;
        let marked = Decodable::decode(d)?;
        let forgotten = Decodable::decode(d)?;
        Ok(Self::from_parts(id, bridges_len, marked, forgotten))
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl<C: AsyncDecodable + Send> AsyncDecodable for bridgetree::Checkpoint<C> {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let id = AsyncDecodable::decode_async(d).await?;
        let bridges_len = AsyncDecodable::decode_async(d).await?;
        let marked = AsyncDecodable::decode_async(d).await?;
        let forgotten = AsyncDecodable::decode_async(d).await?;
        Ok(Self::from_parts(id, bridges_len, marked, forgotten))
    }
}

impl<H: Encodable + Ord + Clone, C: Encodable + Debug, const DEPTH: u8> Encodable
    for bridgetree::BridgeTree<H, C, DEPTH>
{
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += self.prior_bridges().to_vec().encode(s)?;
        len += self.current_bridge().encode(s)?;
        len += self.marked_indices().encode(s)?;
        len += self.checkpoints().encode(s)?;
        len += self.max_checkpoints().encode(s)?;
        Ok(len)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl<
        H: AsyncEncodable + Sync + Send + Ord + Clone,
        C: AsyncEncodable + Sync + Debug,
        const DEPTH: u8,
    > AsyncEncodable for bridgetree::BridgeTree<H, C, DEPTH>
{
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += self.prior_bridges().to_vec().encode_async(s).await?;
        len += self.current_bridge().encode_async(s).await?;
        len += self.marked_indices().encode_async(s).await?;
        len += self.checkpoints().encode_async(s).await?;
        len += self.max_checkpoints().encode_async(s).await?;
        Ok(len)
    }
}

impl<
        H: Decodable + Clone + Ord + bridgetree::Hashable,
        C: Decodable + Clone + Ord + Eq + Debug,
        const DEPTH: u8,
    > Decodable for bridgetree::BridgeTree<H, C, DEPTH>
{
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
        let prior_bridges = Decodable::decode(d)?;
        let current_bridge = Decodable::decode(d)?;
        let saved = Decodable::decode(d)?;
        let checkpoints = Decodable::decode(d)?;
        let max_checkpoints = Decodable::decode(d)?;
        match Self::from_parts(prior_bridges, current_bridge, saved, checkpoints, max_checkpoints) {
            Ok(v) => Ok(v),
            Err(_) => Err(Error::other("BridgeTreeError")),
        }
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl<
        H: AsyncDecodable + Send + Clone + Ord + bridgetree::Hashable,
        C: AsyncDecodable + Send + Clone + Ord + Eq + Debug,
        const DEPTH: u8,
    > AsyncDecodable for bridgetree::BridgeTree<H, C, DEPTH>
{
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let prior_bridges = AsyncDecodable::decode_async(d).await?;
        let current_bridge = AsyncDecodable::decode_async(d).await?;
        let saved = AsyncDecodable::decode_async(d).await?;
        let checkpoints = AsyncDecodable::decode_async(d).await?;
        let max_checkpoints = AsyncDecodable::decode_async(d).await?;
        match Self::from_parts(prior_bridges, current_bridge, saved, checkpoints, max_checkpoints) {
            Ok(v) => Ok(v),
            Err(_) => Err(Error::other("BridgeTreeError")),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{async_trait, deserialize, serialize, SerialDecodable, SerialEncodable};
    use bridgetree::{BridgeTree, Hashable, Level};

    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, SerialEncodable, SerialDecodable)]
    struct Node(String);

    impl Hashable for Node {
        fn empty_leaf() -> Self {
            Self("_".to_string())
        }

        fn combine(_: Level, a: &Self, b: &Self) -> Self {
            Self(a.0.to_string() + &b.0)
        }
    }

    #[test]
    fn serialize_desrialize_inc_merkle_tree() {
        const DEPTH: u8 = 8;

        // Fill the tree with 100 leaves
        let mut tree: BridgeTree<Node, usize, DEPTH> = BridgeTree::new(100);
        for i in 0..100 {
            tree.append(Node(format!("test{}", i)));
            tree.mark();
            tree.checkpoint(i);
        }
        let serial_tree = serialize(&tree);
        let deserial_tree: BridgeTree<Node, usize, DEPTH> = deserialize(&serial_tree).unwrap();

        // Empty tree
        let tree2: BridgeTree<Node, usize, DEPTH> = BridgeTree::new(100);
        let serial_tree2 = serialize(&tree2);
        let deserial_tree2: BridgeTree<Node, usize, DEPTH> = deserialize(&serial_tree2).unwrap();

        // Max leaves
        let mut tree3: BridgeTree<Node, usize, DEPTH> = BridgeTree::new(100);
        for i in 0..2_i32.pow(DEPTH as u32) {
            tree3.append(Node(format!("test{}", i)));
            tree3.mark();
            tree3.checkpoint(i.try_into().unwrap());
        }
        let serial_tree3 = serialize(&tree3);
        let deserial_tree3: BridgeTree<Node, usize, DEPTH> = deserialize(&serial_tree3).unwrap();

        assert!(tree == deserial_tree);
        assert!(tree2 == deserial_tree2);
        assert!(tree3 == deserial_tree3);
    }
}
