/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use std::collections::BTreeMap;

use async_trait::async_trait;
use darkfi_sdk::pasta::pallas;

use std::io::Cursor;

use darkfi_sdk::crypto::{pasta_prelude::FromUniformBytes, poseidon_hash, smt::SmtMemoryFp};
use darkfi_serial::{FutAsyncWriteExt, SerialDecodable, SerialEncodable};
use halo2_proofs::{arithmetic::Field, circuit::Value};
use rand::rngs::OsRng;
use sled_overlay::sled;
use tracing::info;

use crate::{
    event_graph::Event,
    zk::{empty_witnesses, Proof, ProvingKey, VerifyingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};

pub const RLN2_REGISTER_ZKBIN: &[u8] = include_bytes!("proof/rlnv2-diff-register.zk.bin");
pub const RLN2_SIGNAL_ZKBIN: &[u8] = include_bytes!("proof/rlnv2-diff-signal.zk.bin");
pub const RLN2_SLASH_ZKBIN: &[u8] = include_bytes!("proof/rlnv2-diff-slash.zk.bin");

/// RLN epoch genesis in millis
pub const RLN_GENESIS: u64 = 1_738_688_400_000;
/// RLN epoch length in millis
pub const RLN_EPOCH_LEN: u64 = 600_000; // 10 min

/// Hash message/event modulo `Fp`
pub fn hash_event(event: &Event) -> pallas::Base {
    let mut buf = [0u8; 64];
    buf[..blake3::OUT_LEN].copy_from_slice(event.header.id().as_bytes());
    pallas::Base::from_uniform_bytes(&buf)
}

/// Find closest epoch to given timestamp
pub fn closest_epoch(timestamp: u64) -> u64 {
    let time_diff = timestamp - RLN_GENESIS;
    let epoch_idx = time_diff as f64 / RLN_EPOCH_LEN as f64;
    let rounded = epoch_idx.round() as i64;
    RLN_GENESIS + (rounded * RLN_EPOCH_LEN as i64) as u64
}

#[derive(Debug, Clone)]
struct ShareData {
    pub x_shares: Vec<pallas::Base>,
    pub y_shares: Vec<pallas::Base>,
}

impl ShareData {
    fn new() -> Self {
        Self { x_shares: vec![], y_shares: vec![] }
    }
}

#[derive(Debug, Default)]
pub struct MessageMetadata {
    data: BTreeMap<pallas::Base, BTreeMap<pallas::Base, ShareData>>,
}

impl MessageMetadata {
    pub fn new() -> Self {
        Self { data: BTreeMap::new() }
    }

    pub fn add_share(
        &mut self,
        external_nullifier: pallas::Base,
        internal_nullifier: pallas::Base,
        x: pallas::Base,
        y: pallas::Base,
    ) -> Result<()> {
        let inner_map = self.data.entry(external_nullifier).or_insert_with(BTreeMap::new);
        let share_data = inner_map.entry(internal_nullifier).or_insert_with(ShareData::new);

        share_data.x_shares.push(x);
        share_data.y_shares.push(y);

        Ok(())
    }

    pub fn get_shares(
        &self,
        external_nullifier: &pallas::Base,
        internal_nullifier: &pallas::Base,
    ) -> Vec<(pallas::Base, pallas::Base)> {
        if let Some(inner_map) = self.data.get(external_nullifier) {
            if let Some(share_data) = inner_map.get(internal_nullifier) {
                return share_data
                    .x_shares
                    .iter()
                    .cloned()
                    .zip(share_data.y_shares.iter().cloned())
                    .collect()
            }
        }

        vec![]
    }

    /// Check if the recieved message and its metadata are duplicated
    pub fn is_duplicate(
        &self,
        external_nullifier: &pallas::Base,
        internal_nullifier: &pallas::Base,
        x: &pallas::Base,
        y: &pallas::Base,
    ) -> bool {
        if let Some(inner_map) = self.data.get(external_nullifier) {
            if let Some(share_data) = inner_map.get(internal_nullifier) {
                return share_data.x_shares.contains(x) && share_data.y_shares.contains(y);
            }
        }

        false
    }

    /// Check if the message has reused the nullifiers
    pub fn is_reused(
        &self,
        external_nullifier: &pallas::Base,
        internal_nullifier: &pallas::Base,
    ) -> bool {
        if let Some(inner_map) = self.data.get(external_nullifier) {
            return inner_map.get(internal_nullifier).is_some()
        }
        false
    }
}

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub enum RLNNode {
    Registration(pallas::Base),
    Slashing(pallas::Base),
}

pub fn process_commitment(node: RLNNode, identity_tree: &mut SmtMemoryFp) -> Result<()> {
    match node {
        RLNNode::Registration(commitment) => {
            // Add to smt
            let commitment = vec![commitment];
            let commitment: Vec<_> = commitment.into_iter().map(|l| (l, l)).collect();
            identity_tree.insert_batch(commitment)?;
        }
        RLNNode::Slashing(commitment) => {
            // Remove from smt
            let commitment = vec![commitment];
            let commitment: Vec<_> = commitment.into_iter().map(|l| (l, l)).collect();
            identity_tree.remove_leaves(commitment)?;
        }
    }

    Ok(())
}

pub fn create_slash_proof(
    secret: pallas::Base,
    user_msg_limit: u64,
    identities_tree: &mut SmtMemoryFp,
    slash_pk: &ProvingKey,
) -> Result<(Proof, pallas::Base)> {
    let identity_secret_hash = poseidon_hash([secret, user_msg_limit.into()]);
    let commitment = poseidon_hash([identity_secret_hash]);

    let identity_root = identities_tree.root();
    let identity_path = identities_tree.prove_membership(&commitment);
    // TODO: Delete me later
    assert!(identity_path.verify(&identity_root, &commitment, &commitment));

    let witnesses = vec![
        Witness::Base(Value::known(secret)),
        Witness::Base(Value::known(pallas::Base::from(user_msg_limit))),
        Witness::SparseMerklePath(Value::known(identity_path.path)),
    ];

    let public_inputs = vec![secret, pallas::Base::from(user_msg_limit), identity_root];

    let slash_zkbin = ZkBinary::decode(RLN2_SLASH_ZKBIN, false)?;
    let slash_circuit = ZkCircuit::new(witnesses, &slash_zkbin);

    let proof = Proof::create(&slash_pk, &[slash_circuit], &public_inputs, &mut OsRng).unwrap();

    Ok((proof, identity_root))
}

/// Recover secret using Shamir's secret sharing scheme
pub fn sss_recover(shares: &[(pallas::Base, pallas::Base)]) -> pallas::Base {
    let mut secret = pallas::Base::zero();
    for (j, share_j) in shares.iter().enumerate() {
        let mut prod = pallas::Base::one();
        for (i, share_i) in shares.iter().enumerate() {
            if i != j {
                prod *= share_i.0 * (share_i.0 - share_j.0).invert().unwrap();
            }
        }

        prod *= share_j.1;
        secret += prod;
    }

    secret
}

/// Helper function to read or build register verifying key
pub(super) fn build_register_vk(sled_db: &sled::Db) -> Result<VerifyingKey> {
    let register_zkbin = ZkBinary::decode(RLN2_REGISTER_ZKBIN, false).unwrap();
    let register_empty_circuit =
        ZkCircuit::new(empty_witnesses(&register_zkbin).unwrap(), &register_zkbin);

    match sled_db.get("rlnv2-diff-register-vk")? {
        Some(vk) => {
            let mut reader = Cursor::new(vk);
            Ok(VerifyingKey::read(&mut reader, register_empty_circuit)?)
        }
        None => {
            info!(target: "irc::server", "[RLN] Creating RlnV2_Diff_Register VerifyingKey");
            let verifyingkey = VerifyingKey::build(register_zkbin.k, &register_empty_circuit);
            let mut buf = vec![];
            verifyingkey.write(&mut buf)?;
            sled_db.insert("rlnv2-diff-register-vk", buf)?;
            Ok(verifyingkey)
        }
    }
}

/// Helper function to read or build signal verifying key
pub(super) fn build_signal_vk(sled_db: &sled::Db) -> Result<VerifyingKey> {
    let signal_zkbin = ZkBinary::decode(RLN2_SIGNAL_ZKBIN, false).unwrap();
    let signal_empty_circuit =
        ZkCircuit::new(empty_witnesses(&signal_zkbin).unwrap(), &signal_zkbin);

    match sled_db.get("rlnv2-diff-signal-vk")? {
        Some(vk) => {
            let mut reader = Cursor::new(vk);
            Ok(VerifyingKey::read(&mut reader, signal_empty_circuit)?)
        }
        None => {
            info!(target: "irc::server", "[RLN] Creating RlnV2_Diff_Signal VerifyingKey");
            let verifyingkey = VerifyingKey::build(signal_zkbin.k, &signal_empty_circuit);
            let mut buf = vec![];
            verifyingkey.write(&mut buf)?;
            sled_db.insert("rlnv2-diff-signal-vk", buf)?;
            Ok(verifyingkey)
        }
    }
}

/// Helper function to read or build slash proving key
pub(super) fn build_slash_pk(sled_db: &sled::Db) -> Result<ProvingKey> {
    let slash_zkbin = ZkBinary::decode(RLN2_SLASH_ZKBIN, false).unwrap();
    let slash_empty_circuit = ZkCircuit::new(empty_witnesses(&slash_zkbin).unwrap(), &slash_zkbin);

    match sled_db.get("rlnv2-diff-slash-pk")? {
        Some(pk) => {
            let mut reader = Cursor::new(pk);
            Ok(ProvingKey::read(&mut reader, slash_empty_circuit)?)
        }
        None => {
            info!(target: "irc::server", "[RLN] Creating RlnV2_Diff_Slash ProvingKey");
            let provingkey = ProvingKey::build(slash_zkbin.k, &slash_empty_circuit);
            let mut buf = vec![];
            provingkey.write(&mut buf)?;
            sled_db.insert("rlnv2-diff-slash-pk", buf)?;
            Ok(provingkey)
        }
    }
}

/// Helper function to read or build slash verifying key
pub(super) fn build_slash_vk(sled_db: &sled::Db) -> Result<VerifyingKey> {
    let slash_zkbin = ZkBinary::decode(RLN2_SLASH_ZKBIN, false).unwrap();
    let slash_empty_circuit = ZkCircuit::new(empty_witnesses(&slash_zkbin).unwrap(), &slash_zkbin);

    match sled_db.get("rlnv2-diff-slash-vk")? {
        Some(vk) => {
            let mut reader = Cursor::new(vk);
            Ok(VerifyingKey::read(&mut reader, slash_empty_circuit)?)
        }
        None => {
            info!(target: "irc::server", "[RLN] Creating RlnV2_Diff_Slash VerifyingKey");
            let verifyingkey = VerifyingKey::build(slash_zkbin.k, &slash_empty_circuit);
            let mut buf = vec![];
            verifyingkey.write(&mut buf)?;
            sled_db.insert("rlnv2-diff-slash-vk", buf)?;
            Ok(verifyingkey)
        }
    }
}
