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

use std::io::{Error, ErrorKind, Read, Write};

use incrementalmerkletree::Hashable;

use crate::{Decodable, Encodable};

impl Encodable for incrementalmerkletree::Position {
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        u64::from(*self).encode(&mut s)
    }
}

impl Decodable for incrementalmerkletree::Position {
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let dec: u64 = Decodable::decode(&mut d)?;
        Ok(Self::try_from(dec).unwrap())
    }
}

impl<T: Encodable + Ord> Encodable for incrementalmerkletree::bridgetree::Leaf<T> {
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        let mut len = 0;

        match self {
            incrementalmerkletree::bridgetree::Leaf::Left(a) => {
                len += false.encode(&mut s)?;
                len += a.encode(&mut s)?;
            }

            incrementalmerkletree::bridgetree::Leaf::Right(a, b) => {
                len += true.encode(&mut s)?;
                len += a.encode(&mut s)?;
                len += b.encode(&mut s)?;
            }
        }

        Ok(len)
    }
}

impl<T: Decodable + Ord> Decodable for incrementalmerkletree::bridgetree::Leaf<T> {
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let side: bool = Decodable::decode(&mut d)?;

        match side {
            false => {
                let a: T = Decodable::decode(&mut d)?;
                Ok(Self::Left(a))
            }
            true => {
                let a: T = Decodable::decode(&mut d)?;
                let b: T = Decodable::decode(&mut d)?;
                Ok(Self::Right(a, b))
            }
        }
    }
}

impl Encodable for incrementalmerkletree::bridgetree::Checkpoint {
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        let mut len = 0;
        len += self.bridges_len().encode(&mut s)?;
        len += self.is_witnessed().encode(&mut s)?;
        len += self.witnessed().encode(&mut s)?;
        len += self.forgotten().encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for incrementalmerkletree::bridgetree::Checkpoint {
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let bridges_len = Decodable::decode(&mut d)?;
        let is_witnessed = Decodable::decode(&mut d)?;
        let witnessed = Decodable::decode(&mut d)?;
        let forgotten = Decodable::decode(&mut d)?;
        Ok(Self::from_parts(bridges_len, is_witnessed, witnessed, forgotten))
    }
}

impl<T: Encodable + Ord + Clone> Encodable
    for incrementalmerkletree::bridgetree::NonEmptyFrontier<T>
{
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        let mut len = 0;
        len += self.position().encode(&mut s)?;
        len += self.leaf().encode(&mut s)?;
        len += self.ommers().to_vec().encode(&mut s)?;
        Ok(len)
    }
}

impl<T: Decodable + Ord + Clone> Decodable
    for incrementalmerkletree::bridgetree::NonEmptyFrontier<T>
{
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let position = Decodable::decode(&mut d)?;
        let leaf = Decodable::decode(&mut d)?;
        let ommers = Decodable::decode(&mut d)?;

        match Self::from_parts(position, leaf, ommers) {
            Ok(v) => Ok(v),
            Err(_) => Err(Error::new(ErrorKind::Other, "FrontierError")),
        }
    }
}

impl<T: Encodable + Ord + Clone> Encodable for incrementalmerkletree::bridgetree::AuthFragment<T> {
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        let mut len = 0;
        len += self.position().encode(&mut s)?;
        len += self.altitudes_observed().encode(&mut s)?;
        len += self.values().to_vec().encode(&mut s)?;
        Ok(len)
    }
}

impl<T: Decodable + Ord + Clone> Decodable for incrementalmerkletree::bridgetree::AuthFragment<T> {
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let position = Decodable::decode(&mut d)?;
        let altitudes_observed = Decodable::decode(&mut d)?;
        let values = Decodable::decode(&mut d)?;
        Ok(Self::from_parts(position, altitudes_observed, values))
    }
}

impl<T: Encodable + Ord + Clone> Encodable for incrementalmerkletree::bridgetree::MerkleBridge<T> {
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        let mut len = 0;
        len += self.prior_position().encode(&mut s)?;
        len += self.auth_fragments().encode(&mut s)?;
        len += self.frontier().encode(&mut s)?;
        Ok(len)
    }
}

impl<T: Decodable + Ord + Clone> Decodable for incrementalmerkletree::bridgetree::MerkleBridge<T> {
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let prior_position = Decodable::decode(&mut d)?;
        let auth_fragments = Decodable::decode(&mut d)?;
        let frontier = Decodable::decode(&mut d)?;
        Ok(Self::from_parts(prior_position, auth_fragments, frontier))
    }
}

impl<T: Encodable + Ord + Clone, const V: u8> Encodable
    for incrementalmerkletree::bridgetree::BridgeTree<T, V>
{
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        let mut len = 0;
        len += self.prior_bridges().to_vec().encode(&mut s)?;
        len += self.current_bridge().encode(&mut s)?;
        len += self.witnessed_indices().encode(&mut s)?;
        len += self.checkpoints().to_vec().encode(&mut s)?;
        len += self.max_checkpoints().encode(&mut s)?;
        Ok(len)
    }
}

impl<T: Decodable + Ord + Clone + Hashable, const V: u8> Decodable
    for incrementalmerkletree::bridgetree::BridgeTree<T, V>
{
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let prior_bridges = Decodable::decode(&mut d)?;
        let current_bridge = Decodable::decode(&mut d)?;
        let saved = Decodable::decode(&mut d)?;
        let checkpoints = Decodable::decode(&mut d)?;
        let max_checkpoints = Decodable::decode(&mut d)?;
        match Self::from_parts(prior_bridges, current_bridge, saved, checkpoints, max_checkpoints) {
            Ok(v) => Ok(v),
            Err(_) => Err(Error::new(ErrorKind::Other, "BridgeTreeError")),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{deserialize, serialize, SerialDecodable, SerialEncodable};
    use incrementalmerkletree::{bridgetree::BridgeTree, Altitude, Hashable, Tree};

    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, SerialEncodable, SerialDecodable)]
    struct Node(String);

    impl Hashable for Node {
        fn empty_leaf() -> Self {
            Self("_".to_string())
        }

        fn combine(_: Altitude, a: &Self, b: &Self) -> Self {
            Self(a.0.to_string() + &b.0)
        }
    }

    #[test]
    fn serialize_desrialize_inc_merkle_tree() {
        const DEPTH: u8 = 8;

        // Fill the tree with 100 leaves
        let mut tree: BridgeTree<Node, DEPTH> = BridgeTree::new(100);
        for i in 0..100 {
            tree.append(&Node(format!("test{}", i)));
            tree.witness();
            tree.checkpoint();
        }
        let serial_tree = serialize(&tree);
        let deserial_tree: BridgeTree<Node, DEPTH> = deserialize(&serial_tree).unwrap();

        // Empty tree
        let tree2: BridgeTree<Node, DEPTH> = BridgeTree::new(100);
        let serial_tree2 = serialize(&tree2);
        let deserial_tree2: BridgeTree<Node, DEPTH> = deserialize(&serial_tree2).unwrap();

        // Max leaves
        let mut tree3: BridgeTree<Node, DEPTH> = BridgeTree::new(100);
        for i in 0..2_i32.pow(DEPTH as u32) {
            tree3.append(&Node(format!("test{}", i)));
            tree3.witness();
            tree3.checkpoint();
        }
        let serial_tree3 = serialize(&tree3);
        let deserial_tree3: BridgeTree<Node, DEPTH> = deserialize(&serial_tree3).unwrap();

        assert!(tree == deserial_tree);
        assert!(tree2 == deserial_tree2);
        assert!(tree3 == deserial_tree3);
    }
}
