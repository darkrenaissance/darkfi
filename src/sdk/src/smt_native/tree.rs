/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 * Copyright (C) 2021 MONOLOG (Taeho Francis Lim and Jongwhan Lee) MIT License
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

use hashbrown::{HashMap, HashSet};

use super::{
    bits::Bits,
    node::{Node, Unit},
    utils::{get_sorted_indices, slice_to_hash},
    Hash, Proof, HASH_LEN, ROOT_KEY,
};
use crate::GenericResult;

#[derive(Debug)]
pub(crate) struct MemCache {
    pub(crate) set: HashSet<Hash>,
    pub(crate) map: HashMap<Hash, Vec<u8>>,
}

#[allow(dead_code)]
impl MemCache {
    pub(crate) fn new() -> Self {
        Self { set: HashSet::new(), map: HashMap::with_capacity(1 << 12) }
    }

    pub(crate) fn clear(&mut self) {
        self.set.clear();
        self.map.clear();
    }

    pub(crate) fn contains(&self, key: &[u8]) -> bool {
        !self.set.contains(key) && self.map.contains_key(key)
    }

    pub(crate) fn get(&mut self, key: &[u8]) -> Option<Vec<u8>> {
        self.map.get(key).cloned()
    }

    pub(crate) fn put(&mut self, key: &[u8], value: Vec<u8>) {
        self.map.insert(slice_to_hash(key), value);
        self.set.remove(key);
    }

    pub(crate) fn delete(&mut self, key: &[u8]) {
        self.set.insert(slice_to_hash(key));
    }
}

#[derive(Debug)]
pub struct MemoryDb {
    db: HashMap<Hash, Vec<u8>>,
    batch: MemCache,
    batch_on: bool,
}

#[allow(dead_code)]
impl MemoryDb {
    fn new() -> Self {
        Self { db: HashMap::new(), batch: MemCache::new(), batch_on: false }
    }

    fn get(&mut self, key: &[u8]) -> GenericResult<Option<Vec<u8>>> {
        if self.batch_on && self.batch.contains(key) {
            return Ok(self.batch.get(key));
        }

        match self.db.get(key) {
            Some(v) => Ok(Some(v.to_owned())),
            None => Ok(None),
        }
    }

    fn put(&mut self, key: &[u8], value: Vec<u8>) -> GenericResult<()> {
        if self.batch_on {
            self.batch.put(key, value);
        } else {
            self.db.insert(slice_to_hash(key), value);
        }
        Ok(())
    }

    fn delete(&mut self, key: &[u8]) -> GenericResult<()> {
        if self.batch_on {
            self.batch.delete(key);
        } else {
            self.db.remove(key);
        }
        Ok(())
    }

    fn init_batch(&mut self) -> GenericResult<()> {
        if !self.batch_on {
            self.batch.clear();
            self.batch_on = true;
        }
        Ok(())
    }

    fn finish_batch(&mut self) -> GenericResult<()> {
        if self.batch_on {
            for (key, value) in self.batch.map.drain() {
                self.db.insert(key, value);
            }
            for key in self.batch.set.drain() {
                self.db.remove(&key);
            }
            self.batch_on = false;
        }
        Ok(())
    }
}

/// A structure for `monotree`
#[derive(Debug)]
pub struct Monotree {
    db: MemoryDb,
}

impl Default for Monotree {
    fn default() -> Self {
        Self::new()
    }
}

impl Monotree {
    pub fn new() -> Self {
        Self { db: MemoryDb::new() }
    }

    fn hash_digest(bytes: &[u8]) -> Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(bytes);
        let hash = hasher.finalize();
        slice_to_hash(hash.as_bytes())
    }

    /// Retrieves the latest state (root) from the database.
    pub fn get_headroot(&mut self) -> GenericResult<Option<Hash>> {
        let headroot = self.db.get(ROOT_KEY)?;
        match headroot {
            Some(root) => Ok(Some(slice_to_hash(&root))),
            None => Ok(None),
        }
    }

    /// Sets the latest state (root) to the database.
    pub fn set_headroot(&mut self, headroot: Option<&Hash>) {
        if let Some(root) = headroot {
            self.db.put(ROOT_KEY, root.to_vec()).expect("set_headroot(): hash");
        }
    }

    pub fn prepare(&mut self) {
        self.db.init_batch().expect("prepare(): failed to initialize batch");
    }

    pub fn commit(&mut self) {
        self.db.finish_batch().expect("commit(): failed to initialize batch");
    }

    /// Insert key-leaf entry into the tree. Returns a new root hash.
    pub fn insert(
        &mut self,
        root: Option<&Hash>,
        key: &Hash,
        leaf: &Hash,
    ) -> GenericResult<Option<Hash>> {
        match root {
            None => {
                let (hash, bits) = (leaf, Bits::new(key));
                self.put_node(Node::new(Some(Unit { hash, bits }), None))
            }
            Some(root) => self.put(root, Bits::new(key), leaf),
        }
    }

    fn put_node(&mut self, node: Node) -> GenericResult<Option<Hash>> {
        let bytes = node.to_bytes()?;
        let hash = Self::hash_digest(&bytes);
        self.db.put(&hash, bytes)?;
        Ok(Some(hash))
    }

    /// Recursively insert a bytes (in forms of Bits) and a leaf into the tree.
    ///
    /// Optimisation in `monotree` is mainly to compress the path as much as possible
    /// while reducing the number of db accesses using the most intuitive model.
    /// As a result, compared to the standard Sparse Merkle Tree this reduces the
    /// number of DB accesses from `N` to `log2(N)` in both reads and writes.
    ///
    /// Whenever invoked a `put()` call, at least, more than one `put_node()` called,
    /// which triggers a single hash digest + a single DB write.
    /// Compressing the path reduces the number of `put()` calls, which yields reducing
    /// the number of hash function calls as well as the number of DB writes.
    ///
    /// There are four modes when putting the entries and each of them is processed in a
    /// recursive `put()` call.
    /// The number in parenthesis refers to the minimum of DB access and hash fn calls required.
    ///
    /// * set-aside (1)
    ///   Putting the leaf to the next node in the current depth.
    /// * replacement (1)
    ///   Replaces the existing node on the path with the new leaf.
    /// * consume & pass-over (2+)
    ///   Consuming the path on the way, then pass the rest of work to their child node.
    /// * split-node (2)
    ///   Immediately split node into two with the longest common prefix,
    ///   then wind the recursive stack from there returning resulting hashes.
    fn put(&mut self, root: &[u8], bits: Bits, leaf: &[u8]) -> GenericResult<Option<Hash>> {
        let bytes = self.db.get(root)?.expect("bytes");
        let (lc, rc) = Node::cells_from_bytes(&bytes, bits.first())?;
        let unit = lc.as_ref().expect("put(): left-unit");
        let n = Bits::len_common_bits(&unit.bits, &bits);

        match n {
            0 => self.put_node(Node::new(lc, Some(Unit { hash: leaf, bits }))),
            n if n == bits.len() => self.put_node(Node::new(Some(Unit { hash: leaf, bits }), rc)),
            n if n == unit.bits.len() => {
                let hash = &self.put(unit.hash, bits.shift(n, false), leaf)?.expect("put(): hash");

                let unit = unit.to_owned();
                self.put_node(Node::new(Some(Unit { hash, ..unit }), rc))
            }
            _ => {
                let bits = bits.shift(n, false);
                let ru = Unit { hash: leaf, bits };

                let (cloned, unit) = (unit.bits.clone(), unit.to_owned());
                let (hash, bits) = (unit.hash, unit.bits.shift(n, false));
                let lu = Unit { hash, bits };

                // ENFORCE DETERMINISTIC ORDERING
                let (left, right) = if lu.bits < ru.bits { (lu, ru) } else { (ru, lu) };

                let hash =
                    &self.put_node(Node::new(Some(left), Some(right)))?.expect("put(): hash");
                let bits = cloned.shift(n, true);
                self.put_node(Node::new(Some(Unit { hash, bits }), rc))
            }
        }
    }

    /// Get a leaf hash for the given root and key.
    pub fn get(&mut self, root: Option<&Hash>, key: &Hash) -> GenericResult<Option<Hash>> {
        match root {
            None => Ok(None),
            Some(root) => self.find_key(root, Bits::new(key)),
        }
    }

    fn find_key(&mut self, root: &[u8], bits: Bits) -> GenericResult<Option<Hash>> {
        let bytes = self.db.get(root)?.expect("bytes");
        let (cell, _) = Node::cells_from_bytes(&bytes, bits.first())?;
        let unit = cell.as_ref().expect("find_key(): left-unit");
        let n = Bits::len_common_bits(&unit.bits, &bits);
        match n {
            n if n == bits.len() => Ok(Some(slice_to_hash(unit.hash))),
            n if n == unit.bits.len() => self.find_key(unit.hash, bits.shift(n, false)),
            _ => Ok(None),
        }
    }

    /// Remove the given key and its corresponding leaf from the tree. Returns a new root hash.
    pub fn remove(&mut self, root: Option<&Hash>, key: &[u8]) -> GenericResult<Option<Hash>> {
        match root {
            None => Ok(None),
            Some(root) => self.delete_key(root, Bits::new(key)),
        }
    }

    fn delete_key(&mut self, root: &[u8], bits: Bits) -> GenericResult<Option<Hash>> {
        let bytes = self.db.get(root)?.expect("bytes");
        let (lc, rc) = Node::cells_from_bytes(&bytes, bits.first())?;
        let unit = lc.as_ref().expect("delete_key(): left-unit");
        let n = Bits::len_common_bits(&unit.bits, &bits);

        match n {
            n if n == bits.len() => match rc {
                Some(_) => self.put_node(Node::new(None, rc)),
                None => Ok(None),
            },
            n if n == unit.bits.len() => {
                let hash = self.delete_key(unit.hash, bits.shift(n, false))?;
                match (hash, &rc) {
                    (None, None) => Ok(None),
                    (None, Some(_)) => self.put_node(Node::new(None, rc)),
                    (Some(ref hash), _) => {
                        let unit = unit.to_owned();
                        let lc = Some(Unit { hash, ..unit });
                        self.put_node(Node::new(lc, rc))
                    }
                }
            }
            _ => Ok(None),
        }
    }

    /// This method is indented to use the `insert()` method in batch mode.
    /// Note that `inserts()` forces the batch to commit.
    pub fn inserts(
        &mut self,
        root: Option<&Hash>,
        keys: &[Hash],
        leaves: &[Hash],
    ) -> GenericResult<Option<Hash>> {
        let indices = get_sorted_indices(keys, false);
        self.prepare();

        let mut root = root.cloned();
        for i in indices.iter() {
            root = self.insert(root.as_ref(), &keys[*i], &leaves[*i])?;
        }

        self.commit();
        Ok(root)
    }

    /// This method is intended to use the `get()` method in batch mode.
    pub fn gets(&mut self, root: Option<&Hash>, keys: &[Hash]) -> GenericResult<Vec<Option<Hash>>> {
        let mut leaves: Vec<Option<Hash>> = vec![];
        for key in keys.iter() {
            leaves.push(self.get(root, key)?);
        }
        Ok(leaves)
    }

    /// This method is intended to use the `remove()` method in batch mode.
    /// Note that `removes()` forces the batch to commit.
    pub fn removes(&mut self, root: Option<&Hash>, keys: &[Hash]) -> GenericResult<Option<Hash>> {
        let indices = get_sorted_indices(keys, false);
        let mut root = root.cloned();
        self.prepare();

        for i in indices.iter() {
            root = self.remove(root.as_ref(), &keys[*i])?;
        }

        self.commit();
        Ok(root)
    }

    /// Generate a Merkle proof for the given root and key.
    pub fn get_merkle_proof(
        &mut self,
        root: Option<&Hash>,
        key: &[u8],
    ) -> GenericResult<Option<Proof>> {
        let mut proof: Proof = vec![];
        match root {
            None => Ok(None),
            Some(root) => self.gen_proof(root, Bits::new(key), &mut proof),
        }
    }

    fn gen_proof(
        &mut self,
        root: &[u8],
        bits: Bits,
        proof: &mut Proof,
    ) -> GenericResult<Option<Proof>> {
        let bytes = self.db.get(root)?.expect("bytes");
        let (cell, _) = Node::cells_from_bytes(&bytes, bits.first())?;
        let unit = cell.as_ref().expect("gen_proof(): left-unit");
        let n = Bits::len_common_bits(&unit.bits, &bits);

        match n {
            n if n == bits.len() => {
                proof.push(self.encode_proof(&bytes, bits.first())?);
                Ok(Some(proof.to_owned()))
            }
            n if n == unit.bits.len() => {
                proof.push(self.encode_proof(&bytes, bits.first())?);
                self.gen_proof(unit.hash, bits.shift(n, false), proof)
            }
            _ => Ok(None),
        }
    }

    fn encode_proof(&self, bytes: &[u8], right: bool) -> GenericResult<(bool, Vec<u8>)> {
        match Node::from_bytes(bytes)? {
            Node::Soft(_) => Ok((false, bytes[HASH_LEN..].to_vec())),
            Node::Hard(_, _) => {
                if right {
                    Ok((true, [&bytes[..bytes.len() - HASH_LEN - 1], &[0x01]].concat()))
                } else {
                    Ok((false, bytes[HASH_LEN..].to_vec()))
                }
            }
        }
    }
}

/// Verify a MerkleProof with the given root and leaf.
pub fn verify_proof(root: Option<&Hash>, leaf: &Hash, proof: Option<&Proof>) -> bool {
    match proof {
        None => false,
        Some(proof) => {
            let mut hash = leaf.to_owned();
            proof.iter().rev().for_each(|(right, cut)| {
                if *right {
                    let l = cut.len();
                    let o = [&cut[..l - 1], &hash[..], &cut[l - 1..]].concat();
                    hash = Monotree::hash_digest(&o);
                } else {
                    let o = [&hash[..], &cut[..]].concat();
                    hash = Monotree::hash_digest(&o);
                }
            });
            root.expect("verify_proof(): root") == &hash
        }
    }
}
