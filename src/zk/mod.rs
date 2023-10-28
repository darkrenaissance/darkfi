/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

/// Halo2 zkas virtual machine
pub mod vm;
pub use vm::ZkCircuit;

/// VM heap variable definitions and utility functions
pub mod vm_heap;
pub use vm_heap::{empty_witnesses, Witness};

/// ZK gadget implementations
pub mod gadget;

/// Proof creation API
pub mod proof;
pub use proof::{Proof, ProvingKey, VerifyingKey};

/// Trace computation of intermediate values in circuit
mod tracer;
pub use tracer::DebugOpValue;

pub mod halo2 {
    pub use halo2_proofs::{
        arithmetic::Field,
        circuit::{AssignedCell, Layouter, Value},
        dev, plonk,
        plonk::{Advice, Assigned, Column},
    };
}

//pub(in crate::zk) fn assign_free_advice<F: Field, V: Copy>(
pub fn assign_free_advice<F: halo2::Field, V: Copy>(
    mut layouter: impl halo2::Layouter<F>,
    column: halo2::Column<halo2::Advice>,
    value: halo2::Value<V>,
) -> Result<halo2::AssignedCell<V, F>, halo2::plonk::Error>
where
    for<'v> halo2::Assigned<F>: From<&'v V>,
{
    layouter.assign_region(
        || "load private",
        |mut region| region.assign_advice(|| "load private", column, 0, || value),
    )
}

#[cfg(feature = "tinyjson")]
use darkfi_sdk::pasta::pallas;
#[cfg(feature = "tinyjson")]
use std::{collections::HashMap, fs::File, io::Write, path::Path};
#[cfg(feature = "tinyjson")]
use tinyjson::JsonValue::{Array as JsonArray, Object as JsonObj, String as JsonStr};

#[cfg(feature = "tinyjson")]
/// Export witness.json which can be used by zkrunner for debugging circuits
pub fn export_witness_json<P: AsRef<Path>>(
    output_path: P,
    prover_witnesses: &Vec<Witness>,
    public_inputs: &Vec<pallas::Base>,
) {
    let mut witnesses = Vec::new();
    for witness in prover_witnesses {
        let mut value_json = HashMap::new();
        match witness {
            Witness::Base(value) => {
                value.map(|w1| {
                    value_json.insert("Base".to_string(), JsonStr(format!("{:?}", w1)));
                    w1
                });
            }
            Witness::Scalar(value) => {
                value.map(|w1| {
                    value_json.insert("Scalar".to_string(), JsonStr(format!("{:?}", w1)));
                    w1
                });
            }
            _ => unimplemented!(),
        }
        witnesses.push(JsonObj(value_json));
    }

    let mut instances = Vec::new();
    for instance in public_inputs {
        instances.push(JsonStr(format!("{:?}", instance)));
    }

    let witnesses_json = JsonArray(witnesses);
    let instances_json = JsonArray(instances);
    let witness_json = JsonObj(HashMap::from([
        ("witnesses".to_string(), witnesses_json),
        ("instances".to_string(), instances_json),
    ]));
    // This is a debugging method. We don't care about .expect() crashing.
    let json = witness_json.format().expect("cannot create debug json");
    let mut output = File::create(output_path).expect("cannot write file");
    output.write_all(json.as_bytes()).expect("write failed");
}
