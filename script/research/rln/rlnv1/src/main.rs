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

use std::{collections::HashMap, time::Instant};

use darkfi::{
    zk::{empty_witnesses, halo2::Value, Proof, ProvingKey, VerifyingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::{
    crypto::{pasta_prelude::*, poseidon_hash, MerkleNode, MerkleTree},
    pasta::{Ep, Fp},
};
use lazy_static::lazy_static;
use rand::rngs::OsRng;

// These should be unique constants per application.
lazy_static! {
    static ref RLN_IDENTIFIER: Fp = Fp::from(42);
    static ref IDENTITY_DERIVATION_PATH: Fp = Fp::from(11);
    static ref NULLIFIER_DERIVATION_PATH: Fp = Fp::from(12);
}

fn hash_message(message: &[u8]) -> Fp {
    let hasher = Ep::hash_to_curve("rln-domain:demoapp");
    let message_point = hasher(message);
    let message_coords = message_point.to_affine().coordinates().unwrap();
    poseidon_hash([*message_coords.x(), *message_coords.y()])
}

fn sss_recover(shares: &[(Fp, Fp)]) -> Fp {
    let mut secret = Fp::zero();
    for (j, share_j) in shares.iter().enumerate() {
        let mut prod = Fp::one();
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

fn main() {
    let epoch = Fp::from(1674509414);
    let external_nullifier = poseidon_hash([epoch, *RLN_IDENTIFIER]);

    // The identity commitment should be something that cannot be
    // precalculated for usage in the future, and possibly also has
    // to be some kind of puzzle that is costly to (pre)calculate.
    // Alternatively, it could be economic stake of funds which could
    // then be lost if spam is detected and acted upon.
    let secret_key = Fp::random(&mut OsRng);
    let identity_commitment = poseidon_hash([*IDENTITY_DERIVATION_PATH, secret_key]);

    // ============
    // Registration
    // ============
    let mut membership_tree = MerkleTree::new(1);
    let mut identity_roots: Vec<MerkleNode> = vec![];
    let mut banned_roots: Vec<MerkleNode> = vec![];
    let mut identities = HashMap::new();

    // Everyone needs to maintain the leaf positions, because to slash, we
    // need to provide a valid authentication path. Therefore, the easiest
    // way is to store a hashmap.
    assert!(!identities.contains_key(&identity_commitment.to_repr()));
    membership_tree.append(MerkleNode::from(identity_commitment));
    let leaf_pos = membership_tree.mark().unwrap();
    identities.insert(identity_commitment.to_repr(), leaf_pos);
    identity_roots.push(membership_tree.root(0).unwrap());

    // ==========
    // Signalling
    // ==========
    let a_1 = poseidon_hash([secret_key, external_nullifier]);

    // Construct share
    let x = hash_message(b"hello i wanna spam");
    let y = a_1 * x + secret_key;

    // Construct internal nullifier
    let internal_nullifier = poseidon_hash([*NULLIFIER_DERIVATION_PATH, a_1]);

    let identity_root = membership_tree.root(0).unwrap();
    let identity_path = membership_tree.witness(leaf_pos, 0).unwrap();

    // zkSNARK things
    let signal_zkbin = include_bytes!("../signal.zk.bin");
    let rln_zkbin = ZkBinary::decode(signal_zkbin).unwrap();
    let rln_empty_circuit = ZkCircuit::new(empty_witnesses(&rln_zkbin).unwrap(), &rln_zkbin);

    print!("[Interaction] Building Proving key... ");
    let now = Instant::now();
    let rln_pk = ProvingKey::build(13, &rln_empty_circuit);
    println!("[{:?}]", now.elapsed());

    print!("[Interaction] Building Verifying key... ");
    let now = Instant::now();
    let rln_vk = VerifyingKey::build(13, &rln_empty_circuit);
    println!("[{:?}]", now.elapsed());

    // Prover's witnesses and public inputs
    let witnesses = vec![
        Witness::Base(Value::known(secret_key)),
        Witness::MerklePath(Value::known(identity_path.clone().try_into().unwrap())),
        Witness::Uint32(Value::known(u64::from(leaf_pos).try_into().unwrap())),
        Witness::Base(Value::known(x)),
        Witness::Base(Value::known(epoch)),
        Witness::Base(Value::known(*RLN_IDENTIFIER)),
    ];

    let public_inputs = vec![
        epoch,
        *RLN_IDENTIFIER,
        x, // <-- Message hash
        identity_root.inner(),
        internal_nullifier,
        y,
    ];

    // Build a circuit with these witnesses
    print!("[Interaction] Creating ZK proof... ");
    let now = Instant::now();
    let rln_circuit = ZkCircuit::new(witnesses, &rln_zkbin);
    let proof = Proof::create(&rln_pk, &[rln_circuit], &public_inputs, &mut OsRng).unwrap();
    println!("[{:?}]", now.elapsed());

    // ============
    // Verification
    // ============
    print!("[Interaction] Verifying ZK proof... ");
    let now = Instant::now();
    assert!(proof.verify(&rln_vk, &public_inputs).is_ok());
    assert!(!banned_roots.contains(&MerkleNode::from(public_inputs[3])));
    assert!(identity_roots.contains(&MerkleNode::from(public_inputs[3])));
    println!("[{:?}]", now.elapsed());

    // NOTE: These shares should actually be tracked through the internal nullifier.
    let mut shares = vec![(public_inputs[2], public_inputs[5])];

    // Now if another message is sent in the same epoch, we should be able to
    // recover the secret key and ban the sender.
    let x = hash_message(b"hello i'm spamming");
    let y = a_1 * x + secret_key;

    // Same epoch and account, different message
    let witnesses = vec![
        Witness::Base(Value::known(secret_key)),
        Witness::MerklePath(Value::known(identity_path.try_into().unwrap())),
        Witness::Uint32(Value::known(u64::from(leaf_pos).try_into().unwrap())),
        Witness::Base(Value::known(x)),
        Witness::Base(Value::known(epoch)),
        Witness::Base(Value::known(*RLN_IDENTIFIER)),
    ];

    let public_inputs = vec![
        epoch,
        *RLN_IDENTIFIER,
        x, // <-- Message hash
        identity_root.inner(),
        internal_nullifier,
        y,
    ];

    // Build a circuit with these witnesses
    print!("[Interaction] Creating ZK proof... ");
    let now = Instant::now();
    let rln_circuit = ZkCircuit::new(witnesses, &rln_zkbin);
    let proof = Proof::create(&rln_pk, &[rln_circuit], &public_inputs, &mut OsRng).unwrap();
    println!("[{:?}]", now.elapsed());

    print!("[Interaction] Verifying ZK proof... ");
    let now = Instant::now();
    assert!(proof.verify(&rln_vk, &public_inputs).is_ok());
    assert!(!banned_roots.contains(&MerkleNode::from(public_inputs[3])));
    assert!(identity_roots.contains(&MerkleNode::from(public_inputs[3])));
    println!("[{:?}]", now.elapsed());

    // NOTE: These shares should actually be tracked through the internal nullifier.
    shares.push((public_inputs[2], public_inputs[5]));

    // ========
    // Slashing
    // ========

    // We should be able to retrieve the secret key because two messages were
    // sent in the same epoch.
    let recovered_secret = sss_recover(&shares);
    assert_eq!(recovered_secret, secret_key);

    // Create a slash proof
    let slash_zkbin = include_bytes!("../slash.zk.bin");
    let slash_zkbin = ZkBinary::decode(slash_zkbin).unwrap();
    let slash_empty_circuit = ZkCircuit::new(empty_witnesses(&slash_zkbin).unwrap(), &slash_zkbin);

    print!("[Slash] Building Proving key... ");
    let now = Instant::now();
    let slash_pk = ProvingKey::build(13, &slash_empty_circuit);
    println!("[{:?}]", now.elapsed());

    print!("[Slash] Building Verifying key... ");
    let now = Instant::now();
    let slash_vk = VerifyingKey::build(13, &slash_empty_circuit);
    println!("[{:?}]", now.elapsed());

    // Find the leaf position in the hashmap of identity commitments
    let identity_commitment = poseidon_hash([*IDENTITY_DERIVATION_PATH, recovered_secret]);
    let leaf_pos = identities.get(&identity_commitment.to_repr()).unwrap();
    let identity_root = membership_tree.root(0).unwrap();
    let identity_path = membership_tree.witness(*leaf_pos, 0);
    let identity_path = identity_path.unwrap();

    // Witnesses & public inputs
    let witnesses = vec![
        Witness::Base(Value::known(recovered_secret)),
        Witness::MerklePath(Value::known(identity_path.try_into().unwrap())),
        Witness::Uint32(Value::known(u64::from(*leaf_pos).try_into().unwrap())),
    ];

    let public_inputs = vec![identity_root.inner()];

    print!("[Slash] Creating ZK proof... ");
    let now = Instant::now();
    let slash_circuit = ZkCircuit::new(witnesses, &slash_zkbin);
    let proof = Proof::create(&slash_pk, &[slash_circuit], &public_inputs, &mut OsRng).unwrap();
    println!("[{:?}]", now.elapsed());

    print!("[Slash] Verifying ZK proof... ");
    let now = Instant::now();
    assert!(!banned_roots.contains(&MerkleNode::from(public_inputs[0])));
    assert!(identity_roots.contains(&MerkleNode::from(public_inputs[0]))); // <- Will this be true?
    assert!(proof.verify(&slash_vk, &public_inputs).is_ok());
    println!("[{:?}]", now.elapsed());
    banned_roots.push(MerkleNode::from(public_inputs[0]));

    println!("boi u banned");
}
