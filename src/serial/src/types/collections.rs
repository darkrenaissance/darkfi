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

//! Serialization of collections
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    io::{Error, Read, Write},
};

use crate::{Decodable, Encodable, VarInt};

impl<T: Encodable> Encodable for HashSet<T> {
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        let mut len = 0;
        len += VarInt(self.len() as u64).encode(&mut s)?;
        for c in self.iter() {
            len += c.encode(&mut s)?;
        }
        Ok(len)
    }
}

impl<T: Decodable + std::cmp::Eq + std::hash::Hash> Decodable for HashSet<T> {
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let len = VarInt::decode(&mut d)?.0;
        let mut ret = HashSet::new();
        for _ in 0..len {
            let entry: T = Decodable::decode(&mut d)?;
            ret.insert(entry);
        }
        Ok(ret)
    }
}

impl<T: Encodable, U: Encodable> Encodable for BTreeMap<T, U> {
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        let mut len = 0;
        len += VarInt(self.len() as u64).encode(&mut s)?;
        for c in self.iter() {
            len += c.0.encode(&mut s)?;
            len += c.1.encode(&mut s)?;
        }
        Ok(len)
    }
}

impl<T: Decodable + std::cmp::Ord, U: Decodable> Decodable for BTreeMap<T, U> {
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let len = VarInt::decode(&mut d)?.0;
        let mut ret = BTreeMap::new();
        for _ in 0..len {
            let key: T = Decodable::decode(&mut d)?;
            let entry: U = Decodable::decode(&mut d)?;
            ret.insert(key, entry);
        }
        Ok(ret)
    }
}

impl<T: Encodable> Encodable for BTreeSet<T> {
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        let mut len = 0;
        len += VarInt(self.len() as u64).encode(&mut s)?;
        for c in self.iter() {
            len += c.encode(&mut s)?;
        }
        Ok(len)
    }
}

impl<T: Decodable + std::cmp::Ord> Decodable for BTreeSet<T> {
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let len = VarInt::decode(&mut d)?.0;
        let mut ret = BTreeSet::new();
        for _ in 0..len {
            let key: T = Decodable::decode(&mut d)?;
            ret.insert(key);
        }
        Ok(ret)
    }
}

impl<T: Decodable + std::cmp::Eq + std::hash::Hash, U: Decodable> Decodable for HashMap<T, U> {
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let len = VarInt::decode(&mut d)?.0;
        let mut ret = HashMap::new();
        for _ in 0..len {
            let key: T = Decodable::decode(&mut d)?;
            let entry: U = Decodable::decode(&mut d)?;
            ret.insert(key, entry);
        }
        Ok(ret)
    }
}

impl<T: Encodable, U: Encodable> Encodable for HashMap<T, U> {
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        let mut len = 0;
        len += VarInt(self.len() as u64).encode(&mut s)?;
        for c in self.iter() {
            len += c.0.encode(&mut s)?;
            len += c.1.encode(&mut s)?;
        }
        Ok(len)
    }
}
