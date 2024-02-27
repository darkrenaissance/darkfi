/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use std::collections::HashMap;

use darkfi_sdk::{
    crypto::{
        schnorr::{SchnorrPublic, SchnorrSecret, Signature},
        PublicKey, SecretKey,
    },
    dark_tree::{dark_forest_leaf_vec_integrity_check, DarkForest, DarkLeaf, DarkTree},
    error::DarkTreeResult,
    pasta::pallas,
    tx::ContractCall,
};

#[cfg(feature = "async-serial")]
use darkfi_serial::async_trait;

use darkfi_serial::{Encodable, SerialDecodable, SerialEncodable};
use log::{debug, error};

use crate::{
    error::TxVerifyFailed,
    zk::{proof::VerifyingKey, Proof},
    Error, Result,
};

macro_rules! zip {
    ($x:expr) => ($x);
    ($x:expr, $($y:expr), +) => (
        $x.iter().zip(zip!($($y), +))
    )
}

// ANCHOR: transaction
/// A Transaction contains an arbitrary number of `ContractCall` objects,
/// along with corresponding ZK proofs and Schnorr signatures. `DarkLeaf`
/// is used to map relations between contract calls in the transaciton.
#[derive(Clone, Default, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Transaction {
    /// Calls executed in this transaction
    pub calls: Vec<DarkLeaf<ContractCall>>,
    /// Attached ZK proofs
    pub proofs: Vec<Vec<Proof>>,
    /// Attached Schnorr signatures
    pub signatures: Vec<Vec<Signature>>,
}
// ANCHOR_END: transaction

impl Transaction {
    /// Verify ZK proofs for the entire transaction.
    pub async fn verify_zkps(
        &self,
        verifying_keys: &HashMap<[u8; 32], HashMap<String, VerifyingKey>>,
        zkp_table: Vec<Vec<(String, Vec<pallas::Base>)>>,
    ) -> Result<()> {
        // TODO: Are we sure we should assert here?
        assert_eq!(self.calls.len(), self.proofs.len());
        assert_eq!(self.calls.len(), zkp_table.len());

        for (call, (proofs, pubvals)) in zip!(self.calls, self.proofs, zkp_table) {
            assert_eq!(proofs.len(), pubvals.len());

            let Some(contract_map) = verifying_keys.get(&call.data.contract_id.to_bytes()) else {
                error!(
                    target: "tx::verify_zkps",
                    "[TX] Verifying keys not found for contract {}",
                    call.data.contract_id,
                );
                return Err(TxVerifyFailed::InvalidZkProof.into())
            };

            for (proof, (zk_ns, public_vals)) in proofs.iter().zip(pubvals.iter()) {
                if let Some(vk) = contract_map.get(zk_ns) {
                    // We have a verifying key for this
                    debug!(target: "tx::verify_zkps", "[TX] public inputs: {:#?}", public_vals);
                    if let Err(e) = proof.verify(vk, public_vals) {
                        error!(
                            target: "tx::verify_zkps",
                            "[TX] Failed verifying {}::{} ZK proof: {:#?}",
                            call.data.contract_id, zk_ns, e
                        );
                        return Err(TxVerifyFailed::InvalidZkProof.into())
                    }
                    debug!(
                        target: "tx::verify_zkps",
                        "[TX] Successfully verified {}::{} ZK proof",
                        call.data.contract_id, zk_ns,
                    );
                    continue
                }

                error!(
                    target: "tx::verify_zkps",
                    "[TX] {}::{} circuit VK nonexistent",
                    call.data.contract_id, zk_ns,
                );
                return Err(TxVerifyFailed::InvalidZkProof.into())
            }
        }

        Ok(())
    }

    /// Verify Schnorr signatures for the entire transaction.
    pub fn verify_sigs(&self, pub_table: Vec<Vec<PublicKey>>) -> Result<()> {
        // Hash the transaction without the signatures
        let mut hasher = blake3::Hasher::new();
        self.calls.encode(&mut hasher)?;
        self.proofs.encode(&mut hasher)?;
        let data_hash = hasher.finalize();

        debug!(
            target: "tx::verify_sigs",
            "tx.verify_sigs: data_hash: {:?}", data_hash.as_bytes(),
        );

        assert_eq!(self.signatures.len(), pub_table.len());

        for (i, (sigs, pubkeys)) in self.signatures.iter().zip(pub_table.iter()).enumerate() {
            assert_eq!(sigs.len(), pubkeys.len());

            for (pubkey, signature) in pubkeys.iter().zip(sigs) {
                debug!(
                    target: "tx::verify_sigs",
                    "[TX] Verifying signature with public key: {}", pubkey,
                );
                if !pubkey.verify(&data_hash.as_bytes()[..], signature) {
                    error!(
                        target: "tx::verify_sigs",
                        "[TX] tx::verify_sigs[{}] failed to verify signature", i,
                    );
                    return Err(Error::InvalidSignature)
                }
            }

            debug!(target: "tx::verify_sigs", "[TX] tx::verify_sigs[{}] passed", i);
        }

        Ok(())
    }

    /// Create Schnorr signatures for the entire transaction.
    pub fn create_sigs(&self, secret_keys: &[SecretKey]) -> Result<Vec<Signature>> {
        // Hash the transaction without the signatures
        let mut hasher = blake3::Hasher::new();
        self.calls.encode(&mut hasher)?;
        self.proofs.encode(&mut hasher)?;
        let data_hash = hasher.finalize();

        debug!(
            target: "tx::create_sigs",
            "[TX] tx.create_sigs: data_hash: {:?}", data_hash.as_bytes(),
        );

        let mut sigs = vec![];
        for secret in secret_keys {
            debug!(
                target: "tx::create_sigs",
                "[TX] Creating signature with public key: {}", PublicKey::from_secret(*secret),
            );
            let signature = secret.sign(&data_hash.as_bytes()[..]);
            sigs.push(signature);
        }

        Ok(sigs)
    }

    /// Get the transaction hash
    pub fn hash(&self) -> Result<blake3::Hash> {
        let mut hasher = blake3::Hasher::new();
        self.encode(&mut hasher)?;
        Ok(hasher.finalize())
    }
}

// Avoid showing the proofs and sigs in the debug output since often they are very long.
impl std::fmt::Debug for Transaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Transaction {{")?;
        for (i, call) in self.calls.iter().enumerate() {
            writeln!(f, "  Call {} {{", i)?;
            writeln!(f, "    contract_id: {:?}", call.data.contract_id.inner())?;
            let calldata = &call.data.data;
            if !calldata.is_empty() {
                writeln!(f, "    function_code: {}", calldata[0])?;
            }
            writeln!(f, "    parent: {:?}", call.parent_index)?;
            writeln!(f, "    children: {:?}", call.children_indexes)?;
            writeln!(f, "  }},")?;
        }
        writeln!(f, "}}")
    }
}

#[cfg(feature = "net")]
use crate::net::Message;

#[cfg(feature = "net")]
crate::impl_p2p_message!(Transaction, "tx");

/// Calls tree bounds definitions
// TODO: increase min to 2 when fees are implement
pub const MIN_TX_CALLS: usize = 1;
// TODO: verify max value
pub const MAX_TX_CALLS: usize = 20;

/// Auxiliarry structure containing all the information
/// required to execute a contract call.
#[derive(Clone)]
pub struct ContractCallLeaf {
    /// Call executed
    pub call: ContractCall,
    /// Attached ZK proofs
    pub proofs: Vec<Proof>,
}

/// Auxilliary structure to build a full [`Transaction`] using
/// [`DarkTree`] to order everything.
pub struct TransactionBuilder {
    /// Contract calls trees forest
    pub calls: DarkForest<ContractCallLeaf>,
}

// TODO: for now we build the trees manually, but we should
//       add all the proper functions for easier building.
impl TransactionBuilder {
    /// Initialize the builder, using provided data to
    /// generate its initial [`DarkTree`] root.
    pub fn new(
        data: ContractCallLeaf,
        children: Vec<DarkTree<ContractCallLeaf>>,
    ) -> DarkTreeResult<Self> {
        let calls = DarkForest::new(Some(MIN_TX_CALLS), Some(MAX_TX_CALLS));
        let mut self_ = Self { calls };
        self_.append(data, children)?;
        Ok(self_)
    }

    /// Append a new call tree to the forest
    pub fn append(
        &mut self,
        data: ContractCallLeaf,
        children: Vec<DarkTree<ContractCallLeaf>>,
    ) -> DarkTreeResult<()> {
        let tree = DarkTree::new(data, children, None, None);
        self.calls.append(tree)
    }

    /// Builder builds the calls vector using the [`DarkForest`]
    /// and generates the corresponding [`Transaction`].
    pub fn build(&mut self) -> DarkTreeResult<Transaction> {
        // Build the leafs vector
        let leafs = self.calls.build_vec()?;

        // Double check integrity
        dark_forest_leaf_vec_integrity_check(&leafs, Some(MIN_TX_CALLS), Some(MAX_TX_CALLS))?;

        // Build the corresponding transaction
        let mut calls = Vec::with_capacity(leafs.len());
        let mut proofs = Vec::with_capacity(leafs.len());
        for leaf in leafs {
            let call = DarkLeaf {
                data: leaf.data.call,
                parent_index: leaf.parent_index,
                children_indexes: leaf.children_indexes,
            };
            calls.push(call);
            proofs.push(leaf.data.proofs);
        }

        Ok(Transaction { calls, proofs, signatures: vec![] })
    }
}
