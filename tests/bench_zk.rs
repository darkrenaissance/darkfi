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
    Result,
};

const SAMPLES: u128 = 10;

#[test]
#[ignore]
fn bench_zk() -> Result<()> {
    let bincode = include_bytes!("../proof/arithmetic.zk.bin");
    let zkbin = ZkBinary::decode(bincode)?;

    let a = Fp::from(4);
    let b = Fp::from(110);

    // Values for the proof
    let prover_witnesses = vec![Witness::Base(Value::known(a)), Witness::Base(Value::known(b))];

    let public_inputs = vec![a + b, a * b, a - b];

    // I tried cargo bench, but there's no way to display k=X for each individual bench
    // TODO: make a benchmark group and use bench_with_input (cargo bench)
    // see https://github.com/getsentry/relay/blob/master/relay-cardinality/benches/redis_impl.rs#L137-L165
    for k in 11..20 {
        println!("Benchmarking k={}", k);

        let circuit = ZkCircuit::new(prover_witnesses.clone(), &zkbin);
        let proving_key = ProvingKey::build(k, &circuit);
        let mut total = 0;
        for _ in 0..SAMPLES {
            let now = std::time::Instant::now();
            let _ = Proof::create(&proving_key, &[circuit.clone()], &public_inputs, &mut OsRng)?;
            total += now.elapsed().as_millis();
        }
        println!("Avg proving time: {} ms", total / SAMPLES);
        let proof = Proof::create(&proving_key, &[circuit], &public_inputs, &mut OsRng)?;

        let verifier_witnesses = empty_witnesses(&zkbin)?;
        let circuit = ZkCircuit::new(verifier_witnesses, &zkbin);
        let verifying_key = VerifyingKey::build(k, &circuit);
        let mut total = 0;
        for _ in 0..SAMPLES {
            let now = std::time::Instant::now();
            proof.verify(&verifying_key, &public_inputs)?;
            total += now.elapsed().as_millis();
        }
        println!("Avg verification time: {} ms", total / SAMPLES);
    }

    Ok(())
}
