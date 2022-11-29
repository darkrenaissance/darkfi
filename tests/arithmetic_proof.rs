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

use halo2_proofs::circuit::Value;
use pasta_curves::pallas;
use rand::rngs::OsRng;
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};

use darkfi::{
    zk::{
        proof::{ProvingKey, VerifyingKey},
        vm::{Witness, ZkCircuit},
        vm_stack::empty_witnesses,
        Proof,
    },
    zkas::decoder::ZkBinary,
    Result,
};

#[test]
fn arithmetic_proof() -> Result<()> {
    TermLogger::init(LevelFilter::Debug, Config::default(), TerminalMode::Mixed, ColorChoice::Auto)
        .unwrap();

    /* ANCHOR: main */
    let bincode = include_bytes!("../proof/arithmetic.zk.bin");
    let zkbin = ZkBinary::decode(bincode)?;

    // ======
    // Prover
    // ======

    // Witness values
    let a = pallas::Base::from(42);
    let b = pallas::Base::from(69);
    let y_0 = pallas::Base::from(0); // Here we will compare a > b, which is false (0)
    let y_1 = pallas::Base::from(1); // Here we will compare b > a, which is true (1)

    let prover_witnesses = vec![Witness::Base(Value::known(a)), Witness::Base(Value::known(b))];

    // Create the public inputs
    let sum = a + b;
    let product = a * b;
    let difference = a - b;

    let public_inputs = vec![sum, product, difference, y_0, y_1];

    // Create the circuit
    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());

    let proving_key = ProvingKey::build(13, &circuit);
    let proof = Proof::create(&proving_key, &[circuit], &public_inputs, &mut OsRng)?;

    // ========
    // Verifier
    // ========

    // Construct empty witnesses
    let verifier_witnesses = empty_witnesses(&zkbin);

    // Create the circuit
    let circuit = ZkCircuit::new(verifier_witnesses, zkbin);

    let verifying_key = VerifyingKey::build(13, &circuit);
    proof.verify(&verifying_key, &public_inputs)?;
    /* ANCHOR_END: main */

    Ok(())
}
