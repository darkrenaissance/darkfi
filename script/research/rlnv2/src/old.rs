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

use std::{
    collections::BTreeMap,
    time::{Instant, UNIX_EPOCH},
};

use darkfi::{
    zk::{empty_witnesses, halo2::Value, Proof, ProvingKey, VerifyingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::{
    bridgetree::Position,
    crypto::{pasta_prelude::Field, poseidon_hash, MerkleNode, MerkleTree},
    pasta::{group::ff::FromUniformBytes, pallas},
};
use rand::rngs::OsRng;

struct Account {
    identity_nullifier: pallas::Base,
    identity_trapdoor: pallas::Base,
    identity_leaf_pos: Position,
    user_message_limit: pallas::Base,
}

impl Account {
    fn register(
        membership_tree: &mut MerkleTree,
        membership_map: &mut BTreeMap<pallas::Base, Position>,
    ) -> Self {
        let identity_nullifier = pallas::Base::random(&mut OsRng);
        let identity_trapdoor = pallas::Base::random(&mut OsRng);

        let identity_secret_hash = poseidon_hash([identity_nullifier, identity_trapdoor]);
        let user_message_limit = pallas::Base::from(100);
        let identity_commitment = poseidon_hash([identity_secret_hash, user_message_limit]);

        membership_tree.append(MerkleNode::from(identity_commitment));
        let identity_leaf_pos = membership_tree.mark().unwrap();
        membership_map.insert(identity_commitment, identity_leaf_pos);

        Self {
            identity_nullifier,
            identity_trapdoor,
            identity_leaf_pos,
            // message id < user_message_limit
            user_message_limit,
        }
    }
}

/// Hash message modulo Fp
/// In DarkIRC/eventgraph this could be the event ID
fn hash_message(msg: &str) -> pallas::Base {
    let message_hash = blake3::hash(msg.as_bytes());

    let mut buf = [0u8; 64];
    buf[..blake3::OUT_LEN].copy_from_slice(message_hash.as_bytes());
    pallas::Base::from_uniform_bytes(&buf)
}

fn main() {
    // There exists a Merkle tree of identity commitments that serves
    // as the user registry.
    let mut membership_tree = MerkleTree::new(1);
    // Since bridgetree is append-only, we'll maintain a BTreeMap of all the
    // identity commitments and their indexes. Whenever some idenity is banned
    // we'll zero out that leaf and rebuild the bridgetree from the BTreeMap.
    let mut membership_map = BTreeMap::new();

    // Per-app identifier
    let rln_identifier = pallas::Base::from(42);

    // Current epoch
    let epoch = pallas::Base::from(UNIX_EPOCH.elapsed().unwrap().as_secs() as u64);

    // Register account
    let account0 = Account::register(&mut membership_tree, &mut membership_map);

    // ==========
    // Signalling
    // ==========
    let signal_zkbin = include_bytes!("../signal.zk.bin");
    let signal_zkbin = ZkBinary::decode(signal_zkbin).unwrap();
    let signal_empty_circuit =
        ZkCircuit::new(empty_witnesses(&signal_zkbin).unwrap(), &signal_zkbin);

    print!("[Signal] Building Proving key... ");
    let now = Instant::now();
    let signal_pk = ProvingKey::build(signal_zkbin.k, &signal_empty_circuit);
    println!("[{:?}]", now.elapsed());

    print!("[Signal] Building Verifying key... ");
    let now = Instant::now();
    let signal_vk = VerifyingKey::build(signal_zkbin.k, &signal_empty_circuit);
    println!("[{:?}]", now.elapsed());

    // =========================
    // Account 0 sends a message
    // =========================

    // 1. Construct share:
    let message_id = pallas::Base::from(1);
    let external_nullifier = poseidon_hash([epoch, rln_identifier]);
    let a_0 = poseidon_hash([account0.identity_nullifier, account0.identity_trapdoor]);
    let a_1 = poseidon_hash([a_0, external_nullifier, message_id]);
    let x = hash_message("hello i wanna spam");
    let y = a_0 + x * a_1;

    let internal_nullifier = poseidon_hash([a_1]);

    // 2. Create Merkle proof:
    let identity_root = membership_tree.root(0).unwrap();
    let identity_path = membership_tree.witness(account0.identity_leaf_pos, 0).unwrap();

    // 3. Create ZK proof:
    let witnesses = vec![
        Witness::Base(Value::known(account0.identity_nullifier)),
        Witness::Base(Value::known(account0.identity_trapdoor)),
        Witness::MerklePath(Value::known(identity_path.clone().try_into().unwrap())),
        Witness::Uint32(Value::known(u64::from(account0.identity_leaf_pos).try_into().unwrap())),
        Witness::Base(Value::known(x)),
        Witness::Base(Value::known(external_nullifier)),
        Witness::Base(Value::known(message_id)),
        Witness::Base(Value::known(account0.user_message_limit)),
        Witness::Base(Value::known(epoch)),
    ];

    let public_inputs =
        vec![epoch, external_nullifier, x, y, internal_nullifier, identity_root.inner()];

    print!("[Signal] Creating ZK proof for 0:0...");
    let now = Instant::now();
    let signal_circuit = ZkCircuit::new(witnesses, &signal_zkbin);
    let proof = Proof::create(&signal_pk, &[signal_circuit], &public_inputs, &mut OsRng).unwrap();
    println!("[{:?}]", now.elapsed());

    // ============
    // Verification
    // ============
    print!("[Signal] Verifying ZK proof... ");
    let now = Instant::now();
    assert!(proof.verify(&signal_vk, &public_inputs).is_ok());
    println!("[{:?}]", now.elapsed());
}
