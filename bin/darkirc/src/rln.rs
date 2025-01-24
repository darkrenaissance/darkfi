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

//! <https://darkrenaissance.github.io/darkfi/crypto/rln.html>

use darkfi::{
    zk::{empty_witnesses, halo2::Field, ProvingKey, VerifyingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{crypto::MerkleTree, pasta::pallas};
use darkfi_serial::serialize_async;
use log::info;

const RLN_IDENTIFIER: pallas::Base = pallas::Base::from_raw([0, 0, 42, 42]);
const IDENTITY_DERIVATION_PATH: pallas::Base = pallas::Base::from_raw([0, 0, 42, 11]);
const NULLIFIER_DERIVATION_PATH: pallas::Base = pallas::Base::from_raw([0, 0, 42, 12]);

/// Rate-Limit-Nullifiers
///
/// This mechanism is used for spam protection on the IRC network.
pub struct Rln {
    /// DB holding identity commitments and the membership Merkle tree
    /// The scheme is `(k=identity_commitment, v=leaf_position)`
    identities: sled::Tree,
    /// DB holding identity roots
    identity_roots: sled::Tree,
    /// DB holding banned roots
    banned_roots: sled::Tree,
    /// Proving key for the signalling circuit
    signal_pk: ProvingKey,
    /// Verifying key for the signalling circuit
    signal_vk: VerifyingKey,
    /// Proving key for the slashing circuit
    slash_pk: ProvingKey,
    /// Verifying key for the slashing circuit
    slash_vk: VerifyingKey,
}

impl Rln {
    /// Create a new Rln instance
    pub async fn new(sled_db: &sled::Db) -> Result<Self> {
        let identities = sled_db.open_tree("identities")?;
        let identity_roots = sled_db.open_tree("identity_roots")?;
        let banned_roots = sled_db.open_tree("banned_roots")?;

        if !identities.contains_key(b"identity_tree")? {
            info!("Creating RLN membership tree");
            let membership_tree = MerkleTree::new(1);
            identities.insert(b"identity_tree", serialize_async(&membership_tree).await)?;
        }

        let signal_zkbin = include_bytes!("../proof/signal.zk.bin");
        let slash_zkbin = include_bytes!("../proof/slash.zk.bin");

        info!("Building RLN signal proving key");
        let signal_zkbin = ZkBinary::decode(signal_zkbin).unwrap();
        let signal_circuit = ZkCircuit::new(empty_witnesses(&signal_zkbin)?, &signal_zkbin);
        let signal_pk = ProvingKey::build(signal_zkbin.k, &signal_circuit);
        info!("Building RLN signal verifying key");
        let signal_vk = VerifyingKey::build(signal_zkbin.k, &signal_circuit);

        info!("Building RLN slash proving key");
        let slash_zkbin = ZkBinary::decode(slash_zkbin).unwrap();
        let slash_circuit = ZkCircuit::new(empty_witnesses(&slash_zkbin)?, &slash_zkbin);
        let slash_pk = ProvingKey::build(slash_zkbin.k, &slash_circuit);
        info!("Building RLN slash verifying key");
        let slash_vk = VerifyingKey::build(slash_zkbin.k, &slash_circuit);

        Ok(Self {
            identities,
            identity_roots,
            banned_roots,
            signal_pk,
            signal_vk,
            slash_pk,
            slash_vk,
        })
    }

    /// Recover a secret from given secret shares
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
}
