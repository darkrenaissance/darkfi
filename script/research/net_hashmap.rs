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

use std::{
    borrow::Borrow,
    collections::{
        hash_map::{Iter, Keys, Values},
        HashMap,
    },
    hash::Hash,
};

use darkfi_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};

use crate::{net, net::P2pPtr, Result};

/// A general networked hashmap. Propagates changes over P2P.
#[derive(Clone)]
pub struct NetHashMap<K, V> {
    /// The internal [`HashMap`] that represents the actual state
    hashmap: HashMap<K, V>,
    /// Pointer to the P2P network
    p2p: P2pPtr,
}

impl<K, V> NetHashMap<K, V> {
    /// Instantiate a new [`NetHashMap`] with the given [`P2pPtr`]
    pub fn new(p2p: P2pPtr) -> Self {
        let hashmap = HashMap::new();

        Self { hashmap, p2p }
    }
}

impl<K, V> NetHashMap<K, V>
where
    K: Eq + Hash + Send + Sync + Encodable + Decodable + Clone + 'static,
    V: Send + Sync + Encodable + Decodable + Clone + 'static,
{
    /// Returns `true` if the map contains a value for the specified key.
    #[allow(dead_code)]
    pub fn contains_key<Q: ?Sized>(&self, k: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.hashmap.contains_key(k)
    }

    /// Returns `true` if the map contains no elements.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.hashmap.is_empty()
    }

    /// Returns the number of elements in the map.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.hashmap.len()
    }

    /// Insert a key-value pair into the map.
    ///
    /// If the map did not have this key present, `None` is returned.
    ///
    /// If the map did have this key present, the value is updated, and
    /// the old value is returned.
    ///
    /// Additionally, this change will be broadcasted to the P2P network.
    pub async fn insert(&mut self, k: K, v: V) -> Result<Option<V>> {
        let message = NetHashMapInsert { k: k.clone(), v: v.clone() };
        self.p2p.broadcast(&message).await;
        Ok(self.hashmap.insert(k, v))
    }

    /// Removes a key from the map, returning the value at the key if the key
    /// was previously in the map.
    ///
    /// Additionally, this change will be broadcasted to the P2P network.
    pub async fn remove<Q: Encodable + Decodable + ?Sized + Clone>(
        &mut self,
        k: Q,
    ) -> Result<Option<V>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + Send + Sync + Encodable + Decodable + 'static,
    {
        let message = NetHashMapRemove { k: k.clone() };
        self.p2p.broadcast(&message).await;
        Ok(self.hashmap.remove(&k))
    }

    /// An iterator visiting all key-value pairs in arbitrary order.
    /// The iterator element type is `(&'a K, &'a V)`.
    #[allow(dead_code)]
    pub fn iter(&self) -> Iter<'_, K, V> {
        self.hashmap.iter()
    }

    /// An iterator visiting all keys in arbitrary order.
    /// The iterator element type is `&'a K`.
    #[allow(dead_code)]
    pub fn keys(&self) -> Keys<'_, K, V> {
        self.hashmap.keys()
    }

    /// An iterator visiting all values in arbitrary order.
    /// The iterator element type is `&'a V`.
    #[allow(dead_code)]
    pub fn values(&self) -> Values<'_, K, V> {
        self.hashmap.values()
    }
}

#[derive(Debug, Clone, SerialDecodable, SerialEncodable)]
pub struct NetHashMapInsert<K, V> {
    pub k: K,
    pub v: V,
}

impl<K, V> net::Message for NetHashMapInsert<K, V>
where
    K: Encodable + Decodable + Send + Sync + 'static,
    V: Encodable + Decodable + Send + Sync + 'static,
{
    const NAME: &'static str = "nethashmap_insert";
}

#[derive(Debug, Clone, SerialDecodable, SerialEncodable)]
pub struct NetHashMapRemove<K> {
    pub k: K,
}

impl<K> net::Message for NetHashMapRemove<K>
where
    K: Encodable + Decodable + Send + Sync + 'static,
{
    const NAME: &'static str = "nethashmap_remove";
}
