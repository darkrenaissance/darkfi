/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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
use sled_overlay::{sled::Tree, SledDbOverlay};

use super::{
    bits::{merge_owned_and_bits, Bits, BitsOwned},
    node::{Node, Unit},
    utils::{get_sorted_indices, slice_to_hash},
    Hash, Proof, HASH_LEN, ROOT_KEY,
};
use crate::{ContractError, GenericResult};

#[derive(Clone, Debug)]
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

    pub(crate) fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.map.get(key).cloned()
    }

    pub(crate) fn put(&mut self, key: &[u8], value: Vec<u8>) {
        self.map.insert(slice_to_hash(key), value);
        self.set.remove(key);
    }

    pub(crate) fn del(&mut self, key: &[u8]) {
        self.set.insert(slice_to_hash(key));
    }
}

/// Trait for implementing Monotree's storage system
pub trait MonotreeStorageAdapter {
    /// Insert a Key/Value pair into the Monotree
    fn put(&mut self, key: &Hash, value: Vec<u8>) -> GenericResult<()>;
    /// Query the Monotree for a Key
    fn get(&self, key: &[u8]) -> GenericResult<Option<Vec<u8>>>;
    /// Delete an entry in the Monotree
    fn del(&mut self, key: &Hash) -> GenericResult<()>;
    /// Initialize a batch
    fn init_batch(&mut self) -> GenericResult<()>;
    /// Finalize and write a batch
    fn finish_batch(&mut self) -> GenericResult<()>;
}

/// In-memory storage for Monotree
#[derive(Clone, Debug)]
pub struct MemoryDb {
    db: HashMap<Hash, Vec<u8>>,
    batch: MemCache,
    batch_on: bool,
}

#[allow(clippy::new_without_default)]
impl MemoryDb {
    pub fn new() -> Self {
        Self { db: HashMap::new(), batch: MemCache::new(), batch_on: false }
    }
}

impl MonotreeStorageAdapter for MemoryDb {
    fn put(&mut self, key: &Hash, value: Vec<u8>) -> GenericResult<()> {
        if self.batch_on {
            self.batch.put(key, value);
        } else {
            self.db.insert(slice_to_hash(key), value);
        }

        Ok(())
    }

    fn get(&self, key: &[u8]) -> GenericResult<Option<Vec<u8>>> {
        if self.batch_on && self.batch.contains(key) {
            return Ok(self.batch.get(key));
        }

        match self.db.get(key) {
            Some(v) => Ok(Some(v.to_owned())),
            None => Ok(None),
        }
    }

    fn del(&mut self, key: &Hash) -> GenericResult<()> {
        if self.batch_on {
            self.batch.del(key);
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

/// sled-tree based storage for Monotree
#[derive(Clone)]
pub struct SledTreeDb {
    tree: Tree,
    batch: MemCache,
    batch_on: bool,
}

impl SledTreeDb {
    pub fn new(tree: &Tree) -> Self {
        Self { tree: tree.clone(), batch: MemCache::new(), batch_on: false }
    }
}

impl MonotreeStorageAdapter for SledTreeDb {
    fn put(&mut self, key: &Hash, value: Vec<u8>) -> GenericResult<()> {
        if self.batch_on {
            self.batch.put(key, value);
        } else if let Err(e) = self.tree.insert(slice_to_hash(key), value) {
            return Err(ContractError::IoError(e.to_string()))
        }

        Ok(())
    }

    fn get(&self, key: &[u8]) -> GenericResult<Option<Vec<u8>>> {
        if self.batch_on && self.batch.contains(key) {
            return Ok(self.batch.get(key));
        }

        match self.tree.get(key) {
            Ok(Some(v)) => Ok(Some(v.to_vec())),
            Ok(None) => Ok(None),
            Err(e) => Err(ContractError::IoError(e.to_string())),
        }
    }

    fn del(&mut self, key: &Hash) -> GenericResult<()> {
        if self.batch_on {
            self.batch.del(key);
        } else if let Err(e) = self.tree.remove(key) {
            return Err(ContractError::IoError(e.to_string()));
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
                if let Err(e) = self.tree.insert(key, value) {
                    return Err(ContractError::IoError(e.to_string()))
                }
            }
            for key in self.batch.set.drain() {
                if let Err(e) = self.tree.remove(key) {
                    return Err(ContractError::IoError(e.to_string()))
                }
            }
            self.batch_on = false;
        }

        Ok(())
    }
}

/// sled-overlay based storage for Monotree
pub struct SledOverlayDb<'a> {
    overlay: &'a mut SledDbOverlay,
    tree: [u8; 32],
    batch: MemCache,
    batch_on: bool,
}

impl<'a> SledOverlayDb<'a> {
    pub fn new(
        overlay: &'a mut SledDbOverlay,
        tree: &[u8; 32],
    ) -> GenericResult<SledOverlayDb<'a>> {
        if let Err(e) = overlay.open_tree(tree, false) {
            return Err(ContractError::IoError(e.to_string()))
        };
        Ok(Self { overlay, tree: *tree, batch: MemCache::new(), batch_on: false })
    }
}

impl MonotreeStorageAdapter for SledOverlayDb<'_> {
    fn put(&mut self, key: &Hash, value: Vec<u8>) -> GenericResult<()> {
        if self.batch_on {
            self.batch.put(key, value);
        } else if let Err(e) = self.overlay.insert(&self.tree, &slice_to_hash(key), &value) {
            return Err(ContractError::IoError(e.to_string()))
        }

        Ok(())
    }

    fn get(&self, key: &[u8]) -> GenericResult<Option<Vec<u8>>> {
        if self.batch_on && self.batch.contains(key) {
            return Ok(self.batch.get(key));
        }

        match self.overlay.get(&self.tree, key) {
            Ok(Some(v)) => Ok(Some(v.to_vec())),
            Ok(None) => Ok(None),
            Err(e) => Err(ContractError::IoError(e.to_string())),
        }
    }

    fn del(&mut self, key: &Hash) -> GenericResult<()> {
        if self.batch_on {
            self.batch.del(key);
        } else if let Err(e) = self.overlay.remove(&self.tree, key) {
            return Err(ContractError::IoError(e.to_string()));
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
                if let Err(e) = self.overlay.insert(&self.tree, &key, &value) {
                    return Err(ContractError::IoError(e.to_string()))
                }
            }
            for key in self.batch.set.drain() {
                if let Err(e) = self.overlay.remove(&self.tree, &key) {
                    return Err(ContractError::IoError(e.to_string()))
                }
            }
            self.batch_on = false;
        }

        Ok(())
    }
}

/// A structure for `monotree`
///
/// To use this, first create a `MonotreeStorageAdapter` implementor,
/// and then just manually create this struct with `::new()`.
#[derive(Clone, Debug)]
pub struct Monotree<D: MonotreeStorageAdapter> {
    db: D,
}

impl<D: MonotreeStorageAdapter> Monotree<D> {
    pub fn new(db: D) -> Self {
        Self { db }
    }

    fn hash_digest(bytes: &[u8]) -> Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(bytes);
        let hash = hasher.finalize();
        slice_to_hash(hash.as_bytes())
    }

    /// Retrieves the latest state (root) from the database.
    pub fn get_headroot(&self) -> GenericResult<Option<Hash>> {
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

    /// Create and store a soft node using owned bits.
    fn put_soft_node_owned(
        &mut self,
        target_hash: &[u8],
        bits: &BitsOwned,
    ) -> GenericResult<Option<Hash>> {
        let bits_bytes = bits.to_bytes()?;
        let node_bytes = [target_hash, &bits_bytes[..], &[0x00u8]].concat();
        let node_hash = Self::hash_digest(&node_bytes);
        self.db.put(&node_hash, node_bytes)?;
        Ok(Some(node_hash))
    }

    /// Create hard node with owned left bits and preserved right bits (unchanged sibling).
    fn put_hard_node_mixed(
        &mut self,
        left_hash: &[u8],
        left_bits: &BitsOwned,
        right: &Unit,
    ) -> GenericResult<Option<Hash>> {
        let lb_bytes = left_bits.to_bytes()?;
        let rb_bytes = right.bits.to_bytes()?;

        let (lh, lb, rh, rb) = if right.bits.first() {
            (left_hash, &lb_bytes[..], right.hash, &rb_bytes[..])
        } else {
            (right.hash, &rb_bytes[..], left_hash, &lb_bytes[..])
        };

        let node_bytes = [lh, lb, rb, rh, &[0x01u8]].concat();
        let node_hash = Self::hash_digest(&node_bytes);
        self.db.put(&node_hash, node_bytes)?;
        Ok(Some(node_hash))
    }

    /// Collapse a path through soft nodes, accumulating bit prefixes.
    /// Returns `(target_hash, accumulated_bits)`.
    fn collapse_to_target(
        &mut self,
        hash: &[u8],
        prefix: BitsOwned,
    ) -> GenericResult<(Hash, BitsOwned)> {
        let Some(bytes) = self.db.get(hash)? else {
            // Leaf value - return it with the accumulated prefix
            return Ok((slice_to_hash(hash), prefix))
        };

        let node = Node::from_bytes(&bytes)?;
        match node {
            Node::Soft(Some(child)) => {
                let merged = merge_owned_and_bits(&prefix, &child.bits);
                self.collapse_to_target(child.hash, merged)
            }
            Node::Hard(_, _) => Ok((slice_to_hash(hash), prefix)),
            _ => unreachable!("unexpected node type in collapse_to_target"),
        }
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
        let bytes = self.db.get(root)?.expect("put(): bytes");
        let (left, right) = Node::cells_from_bytes(&bytes, bits.first())?;
        let unit = left.as_ref().expect("put(): left-unit");
        let n = Bits::len_common_bits(&unit.bits, &bits);

        match n {
            0 => self.put_node(Node::new(left, Some(Unit { hash: leaf, bits }))),
            n if n == bits.len() => {
                self.put_node(Node::new(Some(Unit { hash: leaf, bits }), right))
            }
            n if n == unit.bits.len() => {
                let hash =
                    &self.put(unit.hash, bits.drop(n), leaf)?.expect("put(): consume & pass-over");

                self.put_node(Node::new(Some(Unit { hash, bits: unit.bits.to_owned() }), right))
            }
            _ => {
                let hash = &self
                    .put_node(Node::new(
                        Some(Unit { hash: unit.hash, bits: unit.bits.drop(n) }),
                        Some(Unit { hash: leaf, bits: bits.drop(n) }),
                    ))?
                    .expect("put(): split-node");

                self.put_node(Node::new(Some(Unit { hash, bits: unit.bits.take(n) }), right))
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
        let bytes = self.db.get(root)?.expect("find_key(): bytes");
        let (cell, _) = Node::cells_from_bytes(&bytes, bits.first())?;
        let unit = cell.as_ref().expect("find_key(): left-unit");
        let n = Bits::len_common_bits(&unit.bits, &bits);
        match n {
            n if n == bits.len() => Ok(Some(slice_to_hash(unit.hash))),
            n if n == unit.bits.len() => self.find_key(unit.hash, bits.drop(n)),
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
        let bytes = self.db.get(root)?.expect("delete_key(): bytes");
        let (left, right) = Node::cells_from_bytes(&bytes, bits.first())?;
        let unit = left.as_ref().expect("delete_key(): left-unit");
        let n = Bits::len_common_bits(&unit.bits, &bits);

        match n {
            // Found the exact key to delete
            n if n == bits.len() => {
                match right {
                    Some(ref sibling) => {
                        // Collapse sibling path through any soft nodes
                        let prefix = sibling.bits.to_bits_owned();
                        let (target, merged_bits) =
                            self.collapse_to_target(sibling.hash, prefix)?;
                        self.put_soft_node_owned(&target, &merged_bits)
                    }
                    None => Ok(None),
                }
            }
            // Recurse into subtree
            n if n == unit.bits.len() => {
                let hash = self.delete_key(unit.hash, bits.drop(n))?;
                match (hash, &right) {
                    (None, None) => Ok(None),

                    (None, Some(sibling)) => {
                        // Child deleted, collapse sibling
                        let prefix = sibling.bits.to_bits_owned();
                        let (target, merged_bits) =
                            self.collapse_to_target(sibling.hash, prefix)?;
                        self.put_soft_node_owned(&target, &merged_bits)
                    }

                    (Some(ref new_child), None) => {
                        // Child modified, no sibling - collapse through
                        let prefix = unit.bits.to_bits_owned();
                        let (target, merged_bits) = self.collapse_to_target(new_child, prefix)?;
                        self.put_soft_node_owned(&target, &merged_bits)
                    }

                    (Some(ref new_child), Some(sibling)) => {
                        // Child modified, sibling exists - check if we need to inline soft node
                        match self.db.get(new_child)? {
                            Some(child_bytes) => {
                                match Node::from_bytes(&child_bytes)? {
                                    Node::Soft(Some(inner)) => {
                                        // Inline the soft node: merge parent bits + soft node bits
                                        let merged = Bits::merge(&unit.bits, &inner.bits);
                                        self.put_hard_node_mixed(inner.hash, &merged, sibling)
                                    }
                                    Node::Hard(_, _) => {
                                        // Can't inline hard node - keep reference
                                        let parent_bits = unit.bits.to_bits_owned();
                                        self.put_hard_node_mixed(new_child, &parent_bits, sibling)
                                    }
                                    _ => unreachable!(),
                                }
                            }
                            None => {
                                // new_child is a leaf valuie
                                let parent_bits = unit.bits.to_bits_owned();
                                self.put_hard_node_mixed(new_child, &parent_bits, sibling)
                            }
                        }
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
        let bytes = self.db.get(root)?.expect("gen_proof(): bytes");
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
                self.gen_proof(unit.hash, bits.drop(n), proof)
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
///
/// NOTE: We use `Monotree::<MemoryDb>` to `hash_digest()` but it doesn't matter.
pub fn verify_proof(root: Option<&Hash>, leaf: &Hash, proof: Option<&Proof>) -> bool {
    match proof {
        None => false,
        Some(proof) => {
            let mut hash = leaf.to_owned();
            proof.iter().rev().for_each(|(right, cut)| {
                if *right {
                    let l = cut.len();
                    let o = [&cut[..l - 1], &hash[..], &cut[l - 1..]].concat();
                    hash = Monotree::<MemoryDb>::hash_digest(&o);
                } else {
                    let o = [&hash[..], &cut[..]].concat();
                    hash = Monotree::<MemoryDb>::hash_digest(&o);
                }
            });
            root.expect("verify_proof(): root") == &hash
        }
    }
}
