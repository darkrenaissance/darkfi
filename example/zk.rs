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
    crypto::pedersen::pedersen_commitment_u64,
    pasta::{
        arithmetic::CurveAffine,
        group::{ff::Field, Curve},
        pallas,
    },
};
use halo2_proofs::circuit::Value;
use rand::rngs::OsRng;

fn main() -> Result<()> {
    let bincode = include_bytes!("simple.zk.bin");
    let zkbin = ZkBinary::decode(bincode)?;

    // ======
    // Prover
    // ======
    // Bigger k = more rows, but slower circuit
    // Number of rows is 2^k
    let k = zkbin.k;

    // Witness values
    let value = 42;
    let value_blind = pallas::Scalar::random(&mut OsRng);

    let prover_witnesses = vec![
        Witness::Base(Value::known(pallas::Base::from(value))),
        Witness::Scalar(Value::known(value_blind)),
    ];

    // Create the public inputs
    let value_commit = pedersen_commitment_u64(value, value_blind);
    let value_coords = value_commit.to_affine().coordinates().unwrap();

    let public_inputs = vec![*value_coords.x(), *value_coords.y()];

    // Create the circuit
    //darkfi::zk::export_witness_json("example/simple.witness.json", &prover_witnesses, &public_inputs);
    let mut circuit = ZkCircuit::new(prover_witnesses, &zkbin.clone());
    circuit.enable_trace();

    let now = std::time::Instant::now();
    let proving_key = ProvingKey::build(k, &circuit);
    println!("ProvingKey built [{} s]", now.elapsed().as_secs_f64());
    let now = std::time::Instant::now();
    let circuits = [circuit];
    let proof = Proof::create(&proving_key, &circuits, &public_inputs, &mut OsRng)?;
    println!("Proof created [{} s]", now.elapsed().as_secs_f64());

    println!("Debug trace:");
    let opvalue_binding = circuits[0].tracer.opvalues.borrow();
    let opvalues = opvalue_binding.as_ref().unwrap();
    for (i, (opcode, opvalue)) in zkbin.opcodes.iter().zip(opvalues.iter()).enumerate() {
        let opcode = opcode.0;
        println!("  {}: {:?} {:?}", i, opcode, opvalue);
    }

    // ========
    // Verifier
    // ========

    // Construct empty witnesses
    let verifier_witnesses = empty_witnesses(&zkbin)?;

    // Create the circuit
    let circuit = ZkCircuit::new(verifier_witnesses, &zkbin);

    let now = std::time::Instant::now();
    let verifying_key = VerifyingKey::build(k, &circuit);
    println!("VerifyingKey built [{} s]", now.elapsed().as_secs_f64());
    let now = std::time::Instant::now();
    proof.verify(&verifying_key, &public_inputs)?;
    println!("proof verify [{} s]", now.elapsed().as_secs_f64());

    Ok(())
}
