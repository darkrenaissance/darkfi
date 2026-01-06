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

use darkfi_sdk::crypto::smt::{MemoryStorageFp, PoseidonFp, SmtMemoryFp, EMPTY_NODES_FP};
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
    let zkbin = ZkBinary::decode(bincode, false)?;

    let hasher = PoseidonFp::new();
    let store = MemoryStorageFp::new();
    let mut smt = SmtMemoryFp::new(store, hasher.clone(), &EMPTY_NODES_FP);

    let leaves = vec![Fp::random(&mut OsRng), Fp::random(&mut OsRng), Fp::random(&mut OsRng)];
    // Use the leaf value as its position in the SMT
    // Therefore we need an additional constraint that leaf == pos
    let leaves: Vec<_> = leaves.into_iter().map(|l| (l, l)).collect();
    smt.insert_batch(leaves.clone()).unwrap();

    let (pos, leaf) = leaves[2];
    assert_eq!(pos, leaf);
    assert_eq!(smt.get_leaf(&pos), leaf);

    let root = smt.root();
    let path = smt.prove_membership(&pos);
    assert!(path.verify(&root, &leaf, &pos));

    // Values for the proof
    let prover_witnesses =
        vec![Witness::SparseMerklePath(Value::known(path.path)), Witness::Base(Value::known(leaf))];

    let public_inputs = vec![root];

    //darkfi::zk::export_witness_json("proof/witness/smt.json", &prover_witnesses, &public_inputs);
    //let (prover_witnesses, public_inputs) = darkfi::zk::import_witness_json("witness.json");
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
