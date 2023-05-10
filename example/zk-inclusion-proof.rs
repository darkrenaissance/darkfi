/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

// ../zkas simple.zk

use darkfi::{
    zk::{
        proof::{Proof, ProvingKey, VerifyingKey},
        vm::{Witness, ZkCircuit},
        vm_heap::empty_witnesses,
    },
    zkas::decoder::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{constants::MERKLE_DEPTH, poseidon_hash, MerkleNode},
    incrementalmerkletree::{bridgetree::BridgeTree, Hashable, Tree},
    pasta::{group::ff::Field, pallas},
};
use darkfi_serial::Encodable;
use halo2_proofs::circuit::Value;
use rand::rngs::OsRng;

type MerkleTree = BridgeTree<MerkleNode, { MERKLE_DEPTH }>;

fn main() -> Result<()> {
    let mut tree = MerkleTree::new(100);

    // Add 10 random things to the tree
    for _ in 0..10 {
        let random_leaf = pallas::Base::random(&mut OsRng);
        let node = MerkleNode::from(random_leaf);
        tree.append(&node);
    }

    let leaf = pallas::Base::random(&mut OsRng);
    let node = MerkleNode::from(leaf);
    tree.append(&node);

    let leaf_position = tree.witness().unwrap();

    // Add 10 more random things to the tree
    for _ in 0..10 {
        let random_leaf = pallas::Base::random(&mut OsRng);
        let node = MerkleNode::from(random_leaf);
        tree.append(&node);
    }

    let root = tree.root(0).unwrap();

    // Now begin zk proof API

    let bincode = include_bytes!("../proof/inclusion_proof.zk.bin");
    let zkbin = ZkBinary::decode(bincode)?;

    // ======
    // Prover
    // ======
    // Bigger k = more rows, but slower circuit
    // Number of rows is 2^k
    let k = 11;
    println!("k = {}", k);

    // Witness values
    let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
    let leaf_position: u64 = leaf_position.into();
    let blind = pallas::Base::random(&mut OsRng);

    let prover_witnesses = vec![
        Witness::Base(Value::known(leaf)),
        Witness::Uint32(Value::known(leaf_position.try_into().unwrap())),
        Witness::MerklePath(Value::known(merkle_path.clone().try_into().unwrap())),
        Witness::Base(Value::known(blind)),
    ];

    // Create the public inputs
    let merkle_root = {
        let position: u64 = leaf_position.into();
        let mut current = MerkleNode::from(leaf);
        for (level, sibling) in merkle_path.iter().enumerate() {
            let level = level as u8;
            current = if position & (1 << level) == 0 {
                MerkleNode::combine(level.into(), &current, sibling)
            } else {
                MerkleNode::combine(level.into(), sibling, &current)
            };
        }
        current
    };

    let enc_leaf = poseidon_hash::<2>([leaf, blind]);
    let public_inputs = vec![merkle_root.inner(), enc_leaf];

    // Create the circuit
    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());

    let now = std::time::Instant::now();
    let proving_key = ProvingKey::build(k, &circuit);
    println!("ProvingKey built [{} s]", now.elapsed().as_secs_f64());
    let now = std::time::Instant::now();
    let proof = Proof::create(&proving_key, &[circuit], &public_inputs, &mut OsRng)?;
    println!("Proof created [{} s]", now.elapsed().as_secs_f64());

    // ========
    // Verifier
    // ========

    // Construct empty witnesses
    let verifier_witnesses = empty_witnesses(&zkbin);

    // Create the circuit
    let circuit = ZkCircuit::new(verifier_witnesses, zkbin);

    let now = std::time::Instant::now();
    let verifying_key = VerifyingKey::build(k, &circuit);
    println!("VerifyingKey built [{} s]", now.elapsed().as_secs_f64());
    let now = std::time::Instant::now();
    proof.verify(&verifying_key, &public_inputs)?;
    println!("proof verify [{} s]", now.elapsed().as_secs_f64());

    let mut data = vec![];
    proof.encode(&mut data)?;
    println!("proof size: {}", data.len());

    Ok(())
}
