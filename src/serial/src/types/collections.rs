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

#[cfg(feature = "fxhash")]
impl<T: Encodable, U: Encodable> Encodable for fxhash::FxHashMap<T, U> {
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

#[cfg(feature = "fxhash")]
impl<T: Decodable + std::cmp::Eq + std::hash::Hash, U: Decodable> Decodable
    for fxhash::FxHashMap<T, U>
{
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let len = VarInt::decode(&mut d)?.0;
        let mut ret = fxhash::FxHashMap::default();
        for _ in 0..len {
            let key: T = Decodable::decode(&mut d)?;
            let entry: U = Decodable::decode(&mut d)?;
            ret.insert(key, entry);
        }
        Ok(ret)
    }
}
