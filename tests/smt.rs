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

use darkfi_sdk::crypto::{
    constants::SPARSE_MERKLE_DEPTH,
    smt::{Poseidon, SparseMerkleTree},
};
use halo2_proofs::{arithmetic::Field, circuit::Value, dev::MockProver, pasta::Fp};
use rand::rngs::OsRng;

use darkfi::{
    zk::{
        proof::{ProvingKey, VerifyingKey},
        vm::ZkCircuit,
        vm_heap::{empty_witnesses, Witness},
        Proof,
    },
    zkas::ZkBinary,
    Result,
};

#[test]
fn zkvm_smt() -> Result<()> {
    let bincode = include_bytes!("../proof/smt.zk.bin");
    let zkbin = ZkBinary::decode(bincode)?;

    let poseidon = Poseidon::<Fp, 2>::new();
    let empty_leaf = [0u8; 32];
    let leaves = [Fp::random(&mut OsRng), Fp::random(&mut OsRng), Fp::random(&mut OsRng)];

    let smt = SparseMerkleTree::<Fp, Poseidon<Fp, 2>, SPARSE_MERKLE_DEPTH>::new_sequential(
        &leaves,
        &poseidon.clone(),
        empty_leaf,
    )
    .unwrap();

    let path = smt.generate_membership_proof(0);
    let root = path.calculate_root(&leaves[0], &poseidon).unwrap();

    let mut witnessed_path = [(Value::unknown(), Value::unknown()); SPARSE_MERKLE_DEPTH];
    for (i, (left, right)) in path.path.into_iter().enumerate() {
        witnessed_path[i] = (Value::known(left), Value::known(right));
    }
    let path = witnessed_path;

    // Values for the proof
    let prover_witnesses = vec![
        Witness::Base(Value::known(root)),
        Witness::SparseMerklePath(path),
        Witness::Base(Value::known(leaves[0])),
    ];

    let public_inputs = vec![root];

    let circuit = ZkCircuit::new(prover_witnesses, &zkbin);

    let mockprover = MockProver::run(zkbin.k, &circuit, vec![public_inputs.clone()])?;
    mockprover.assert_satisfied();

    let proving_key = ProvingKey::build(zkbin.k, &circuit);
    let proof = Proof::create(&proving_key, &[circuit], &public_inputs, &mut OsRng)?;

    let verifier_witnesses = empty_witnesses(&zkbin)?;
    let circuit = ZkCircuit::new(verifier_witnesses, &zkbin);
    let verifying_key = VerifyingKey::build(zkbin.k, &circuit);
    proof.verify(&verifying_key, &public_inputs)?;

    Ok(())
}
