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

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use halo2_proofs::{circuit::Value, pasta::Fp};
use rand::rngs::OsRng;

use darkfi::{
    zk::{
        proof::{ProvingKey, VerifyingKey},
        vm::ZkCircuit,
        vm_heap::{empty_witnesses, Witness},
        Proof,
    },
    zkas::ZkBinary,
};

fn zk_arith(c: &mut Criterion) {
    let bincode = include_bytes!("../proof/arithmetic.zk.bin");
    let zkbin = ZkBinary::decode(bincode).unwrap();

    let a = Fp::from(4);
    let b = Fp::from(110);

    let prover_witnesses = vec![Witness::Base(Value::known(a)), Witness::Base(Value::known(b))];
    let public_inputs = vec![a + b, a * b, a - b];

    //darkfi::zk::export_witness_json("proof/witness/arithmetic.json", &prover_witnesses, &public_inputs);
    let circuit = ZkCircuit::new(prover_witnesses.clone(), &zkbin);

    let mut prove_group = c.benchmark_group("prove");
    prove_group.significance_level(0.01).sample_size(10);
    for k in zkbin.k..20 {
        let proving_key = ProvingKey::build(k, &circuit.clone());
        prove_group.bench_with_input(BenchmarkId::from_parameter(k), &k, |b, &_k| {
            b.iter(|| Proof::create(&proving_key, &[circuit.clone()], &public_inputs, &mut OsRng))
        });
    }
    prove_group.finish();

    let mut verif_group = c.benchmark_group("verify");
    verif_group.significance_level(0.01).sample_size(10);
    for k in zkbin.k..20 {
        let proving_key = ProvingKey::build(k, &circuit.clone());
        let proof =
            Proof::create(&proving_key, &[circuit.clone()], &public_inputs, &mut OsRng).unwrap();
        let verifier_witnesses = empty_witnesses(&zkbin).unwrap();
        let circuit = ZkCircuit::new(verifier_witnesses, &zkbin);
        let verifying_key = VerifyingKey::build(k, &circuit);

        verif_group.bench_with_input(BenchmarkId::from_parameter(k), &k, |b, &_k| {
            b.iter(|| proof.verify(&verifying_key, &public_inputs))
        });
    }
    verif_group.finish();
}

criterion_group!(bench, zk_arith);
criterion_main!(bench);
