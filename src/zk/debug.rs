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

use darkfi_sdk::pasta::pallas;
use log::error;

#[cfg(feature = "tinyjson")]
use {
    std::{collections::HashMap, fs::File, io::Write, path::Path},
    tinyjson::JsonValue::{Array as JsonArray, Object as JsonObj, String as JsonStr},
};

use super::{Witness, ZkCircuit};
use crate::{zkas, Error, Result};

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

/// Call this before `Proof::create()` to perform type checks on the witnesses and check
/// the amount of provided instances are correct.
pub fn zkas_type_checks(
    circuit: &ZkCircuit,
    binary: &zkas::ZkBinary,
    instances: &[pallas::Base],
) -> Result<()> {
    if circuit.witnesses.len() != binary.witnesses.len() {
        error!(
            "Wrong number of witnesses. Should be {}, but instead got {}.",
            binary.witnesses.len(),
            circuit.witnesses.len()
        );
        return Err(Error::WrongWitnessesCount)
    }

    for (i, (circuit_witness, binary_witness)) in
        circuit.witnesses.iter().zip(binary.witnesses.iter()).enumerate()
    {
        let is_pass = match circuit_witness {
            Witness::EcPoint(_) => *binary_witness == zkas::VarType::EcPoint,
            Witness::EcNiPoint(_) => *binary_witness == zkas::VarType::EcNiPoint,
            Witness::EcFixedPoint(_) => *binary_witness == zkas::VarType::EcFixedPoint,
            Witness::Base(_) => *binary_witness == zkas::VarType::Base,
            Witness::Scalar(_) => *binary_witness == zkas::VarType::Scalar,
            Witness::MerklePath(_) => *binary_witness == zkas::VarType::MerklePath,
            Witness::Uint32(_) => *binary_witness == zkas::VarType::Uint32,
            Witness::Uint64(_) => *binary_witness == zkas::VarType::Uint64,
        };
        if !is_pass {
            error!(
                "Wrong witness type at index {}. Expected '{}', but instead got '{}'.",
                i,
                binary_witness.name(),
                circuit_witness.name()
            );
            return Err(Error::WrongWitnessType(i))
        }
    }

    // Count number of public instances
    let mut instances_count = 0;
    for opcode in &circuit.opcodes {
        if let (zkas::Opcode::ConstrainInstance, _) = opcode {
            instances_count += 1;
        }
    }
    if instances.len() != instances_count {
        error!(
            "Wrong number of public inputs. Should be {}, but instead got {}.",
            instances_count,
            instances.len()
        );
        return Err(Error::WrongPublicInputsCount)
    }
    Ok(())
}
