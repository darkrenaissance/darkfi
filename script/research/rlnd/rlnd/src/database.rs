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

use darkfi::{util::path::expand_path, Error, Result};
use darkfi_sdk::{
    bridgetree::Position,
    crypto::{pasta_prelude::PrimeField, MerkleNode, MerkleTree},
    pasta::pallas,
};
use darkfi_serial::{async_trait, deserialize, serialize, SerialDecodable, SerialEncodable};
use sled_overlay::sled;

/// This struct represents a tuple of the form (id, stake).
#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Membership {
    /// Membership leaf position in the memberships Merkle tree
    pub leaf_position: Position,
    /// Member stake value
    pub stake: u64,
}

impl Membership {
    /// Generate a new `Membership` in the memberships tree for provided id and stake.
    pub fn new(memberships_tree: &mut MerkleTree, id: pallas::Base, stake: u64) -> Self {
        memberships_tree.append(MerkleNode::from(id));
        let leaf_position = memberships_tree.mark().unwrap();
        Self { leaf_position, stake }
    }
}

pub const SLED_MAIN_TREE: &[u8] = b"_main";
pub const SLED_MAIN_TREE_MEMBERSHIPS_TREE_KEY: &[u8] = b"_memberships_tree";
pub const SLED_MEMBERSHIP_TREE: &[u8] = b"_memberships";

/// Structure holding all sled trees for DarkFi RLN state management.
#[derive(Clone)]
pub struct RlndDatabase {
    /// Main pointer to the sled db connection
    pub sled_db: sled::Db,
    /// Main `sled` tree, storing arbitrary data,
    /// like the memberships Merkle tree.
    pub main: sled::Tree,
    /// The `sled` tree storing all the memberships information,
    /// where the key is the membership id, and the value is the serialized
    /// structure itself.
    pub membership: sled::Tree,
}

impl RlndDatabase {
    /// Instantiate a new `RlndDatabase`.
    pub fn new(db_path: &str) -> Result<Self> {
        // Initialize or open sled database
        let db_path = expand_path(db_path)?;
        let sled_db = sled_overlay::sled::open(&db_path)?;

        // Open the database trees
        let main = sled_db.open_tree(SLED_MAIN_TREE)?;
        let membership = sled_db.open_tree(SLED_MEMBERSHIP_TREE)?;

        // Check if memberships Merkle tree is initialized
        if main.get(SLED_MAIN_TREE_MEMBERSHIPS_TREE_KEY)?.is_none() {
            main.insert(SLED_MAIN_TREE_MEMBERSHIPS_TREE_KEY, serialize(&MerkleTree::new(1)))?;
        }

        Ok(Self { sled_db, main, membership })
    }

    /// Retrieve memberships Merkle tree record from the main tree.
    pub fn get_memberships_tree(&self) -> Result<MerkleTree> {
        let merkle_tree_bytes = self.main.get(SLED_MAIN_TREE_MEMBERSHIPS_TREE_KEY)?;
        let merkle_tree = deserialize(&merkle_tree_bytes.unwrap())?;
        Ok(merkle_tree)
    }

    /// Generate a new `Membership` record for provided id and stake
    /// and instert it into the database.
    pub fn add_membership(&self, id: pallas::Base, stake: u64) -> Result<Membership> {
        // Grab the memberships Merkle tree
        let mut memberships_merkle_tree = self.get_memberships_tree()?;

        // Generate the new `Membership`
        let membership = Membership::new(&mut memberships_merkle_tree, id, stake);

        // Update the memberships Merkle tree record in the database
        self.main
            .insert(SLED_MAIN_TREE_MEMBERSHIPS_TREE_KEY, serialize(&memberships_merkle_tree))?;

        // Store the new membership
        self.membership.insert(id.to_repr(), serialize(&membership))?;

        Ok(membership)
    }

    /// Retrieve `Membership` by given id.
    pub fn get_membership_by_id(&self, id: &pallas::Base) -> Result<Membership> {
        let Some(found) = self.membership.get(id.to_repr())? else {
            return Err(Error::DatabaseError(format!("Membership was not found for id: {id:?}")))
        };
        let membership = deserialize(&found)?;
        Ok(membership)
    }

    /// Retrieve all membership records contained in the database.
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<(pallas::Base, Membership)>> {
        let mut memberships = vec![];

        for record in self.membership.iter() {
            let (key, membership) = record?;
            let id = convert_pallas_key(&key)?;
            let membership = deserialize(&membership)?;
            memberships.push((id, membership));
        }

        Ok(memberships)
    }

    /// Remove `Membership` record of given id and rebuild the memberships Merkle tree.
    pub fn remove_membership_by_id(&self, id: &pallas::Base) -> Result<Membership> {
        // TODO: add a mutex guard here
        // Remove membership record
        let Some(found) = self.membership.remove(id.to_repr())? else {
            return Err(Error::DatabaseError(format!("Membership was not found for id: {id:?}")))
        };

        // Rebuild the memberships Merkle tree
        self.rebuild_memberships_merkle_tree()?;

        Ok(deserialize(&found)?)
    }

    /// Auxiliary function to rebuild the memberships Merkle tree in the database.
    pub fn rebuild_memberships_merkle_tree(&self) -> Result<()> {
        // Create a new Merkle tree
        let mut memberships_merkle_tree = MerkleTree::new(1);

        // Iterate over keys and generate the new memberships
        let mut memberships = vec![];
        for record in self.membership.iter() {
            let (key, membership) = record?;
            let id = convert_pallas_key(&key)?;
            let membership: Membership = deserialize(&membership)?;
            let membership = Membership::new(&mut memberships_merkle_tree, id, membership.stake);
            memberships.push((id, membership));
        }

        // Update the memberships Merkle tree record in the database
        self.main
            .insert(SLED_MAIN_TREE_MEMBERSHIPS_TREE_KEY, serialize(&memberships_merkle_tree))?;

        // Store the updated memberships
        for (id, membership) in memberships {
            self.membership.insert(id.to_repr(), serialize(&membership))?;
        }

        Ok(())
    }

    /// Retrieve stored memberships count.
    pub fn len(&self) -> usize {
        self.membership.len()
    }

    /// Check if database contains any memberships.
    pub fn is_empty(&self) -> bool {
        self.membership.is_empty()
    }
}

/// Auxiliary function to convert a `pallas::Base` key from an `IVec`.
fn convert_pallas_key(key: &sled::IVec) -> Result<pallas::Base> {
    let mut repr = [0; 32];
    repr.copy_from_slice(key);
    match pallas::Base::from_repr(repr).into_option() {
        Some(key) => Ok(key),
        None => Err(Error::DatabaseError(format!(
            "Key could not be converted into pallas::Base: {key:?}"
        ))),
    }
}
