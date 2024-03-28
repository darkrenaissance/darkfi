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

use criterion::{criterion_group, criterion_main, Criterion};
use rand::rngs::OsRng;
use std::{fs::File, io::Read};

use darkfi::{
    zk::{
        proof::{ProvingKey, VerifyingKey},
        vm::ZkCircuit,
        vm_heap::empty_witnesses,
        Proof,
    },
    zkas::ZkBinary,
};

// Use a witness.json file to benchmark a ZK file
fn zk_from_json(c: &mut Criterion) {
    #[rustfmt::skip]
    let tests = [
        //(
        //    "Opcodes",
        //    "proof/opcodes.zk.bin",
        //    "proof/witness/opcodes.json"
        //),
        //(
        //    "Arithmetic",
        //    "proof/arithmetic.zk.bin",
        //    "proof/witness/arithmetic.json"
        //),
        //(
        //    "SMT",
        //    "proof/smt.zk.bin",
        //    "proof/witness/smt.json"
        //),
        //(
        //    "DAO::mint",
        //    "src/contract/dao/proof/mint.zk.bin",
        //    "src/contract/dao/proof/witness/mint.json"
        //),
        (
            "DAO::propose-input",
            "src/contract/dao/proof/propose-input.zk.bin",
            "src/contract/dao/proof/witness/propose-input.json"
        ),
        //(
        //    "DAO::propose",
        //    "src/contract/dao/proof/propose-main.zk.bin",
        //    "src/contract/dao/proof/witness/propose-main.json"
        //),
        (
            "DAO::vote-input",
            "src/contract/dao/proof/vote-input.zk.bin",
            "src/contract/dao/proof/witness/vote-input.json"
        ),
        //(
        //    "DAO::vote",
        //    "src/contract/dao/proof/vote-main.zk.bin",
        //    "src/contract/dao/proof/witness/vote-main.json"
        //),
        //(
        //    "DAO::exec",
        //    "src/contract/dao/proof/exec.zk.bin",
        //    "src/contract/dao/proof/witness/exec.json"
        //),
        //(
        //    "DAO::auth_xfer-coin",
        //    "src/contract/dao/proof/auth-money-transfer-enc-coin.zk.bin",
        //    "src/contract/dao/proof/witness/auth-money-transfer-enc-coin.json"
        //),
        //(
        //    "DAO::auth_xfer",
        //    "src/contract/dao/proof/auth-money-transfer.zk.bin",
        //    "src/contract/dao/proof/witness/auth-money-transfer.json"
        //),
        //(
        //    "Money::xfer-mint",
        //    "src/contract/money/proof/mint_v1.zk.bin",
        //    "src/contract/money/proof/witness/mint_v1.json"
        //),
        //(
        //    "Money::xfer-burn",
        //    "src/contract/money/proof/burn_v1.zk.bin",
        //    "src/contract/money/proof/witness/burn_v1.json"
        //),
        //(
        //    "Money::fee",
        //    "src/contract/money/proof/fee_v1.zk.bin",
        //    "src/contract/money/proof/witness/fee_v1.json"
        //),
        //(
        //    "Money::auth_token-mint",
        //    "src/contract/money/proof/auth_token_mint_v1.zk.bin",
        //    "src/contract/money/proof/witness/auth_token_mint_v1.json"
        //),
        //(
        //    "Money::token-mint",
        //    "src/contract/money/proof/token_mint_v1.zk.bin",
        //    "src/contract/money/proof/witness/token_mint_v1.json"
        //),
        //(
        //    "Money::token-freeze",
        //    "src/contract/money/proof/token_freeze_v1.zk.bin",
        //    "src/contract/money/proof/witness/token_freeze_v1.json"
        //),
    ];

    println!("Running ZK Json benchmarks");
    for (name, proof, witness) in &tests {
        println!("Benchmarking '{}': {} {}", name, proof, witness);
        run_benchmark(c, name, proof, witness);
        println!("Done!");
    }
}

fn run_benchmark(c: &mut Criterion, name: &str, proof: &str, witness: &str) {
    let mut bincode = Vec::new();
    let mut f = File::open(proof).unwrap();
    f.read_to_end(&mut bincode).unwrap();
    let zkbin = ZkBinary::decode(&bincode).unwrap();

    let (prover_witnesses, public_inputs) = darkfi::zk::import_witness_json(witness);
    let circuit = ZkCircuit::new(prover_witnesses.clone(), &zkbin);

    let proving_key = ProvingKey::build(zkbin.k, &circuit.clone());
    c.bench_function(&format!("prove {}", name), |b| {
        b.iter(|| Proof::create(&proving_key, &[circuit.clone()], &public_inputs, &mut OsRng))
    });

    let proof =
        Proof::create(&proving_key, &[circuit.clone()], &public_inputs, &mut OsRng).unwrap();
    let verifier_witnesses = empty_witnesses(&zkbin).unwrap();
    let circuit = ZkCircuit::new(verifier_witnesses, &zkbin);
    let verifying_key = VerifyingKey::build(zkbin.k, &circuit);

    c.bench_function(&format!("verify {}", name), |b| {
        b.iter(|| proof.verify(&verifying_key, &public_inputs))
    });
}

criterion_group!(bench, zk_from_json);
criterion_main!(bench);
