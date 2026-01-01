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

#[cfg(feature = "tinyjson")]
use {
    super::halo2::Value,
    darkfi_sdk::crypto::{pasta_prelude::*, util::FieldElemAsStr, MerkleNode},
    std::{
        collections::HashMap,
        fs::File,
        io::{Read, Write},
        path::Path,
    },
    tinyjson::JsonValue::{
        self, Array as JsonArray, Number as JsonNum, Object as JsonObj, String as JsonStr,
    },
};

use darkfi_sdk::pasta::pallas;
use tracing::error;

use super::{Witness, ZkCircuit};
use crate::{zkas, Error, Result};

#[cfg(feature = "tinyjson")]
/// Export witness.json which can be used by zkrunner for debugging circuits
/// Note that this function makes liberal use of unwraps so it could panic.
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
                value.map(|w| {
                    value_json.insert("Base".to_string(), JsonStr(w.to_string()));
                    w
                });
            }
            Witness::Scalar(value) => {
                value.map(|w| {
                    value_json.insert("Scalar".to_string(), JsonStr(w.to_string()));
                    w
                });
            }
            Witness::Uint32(value) => {
                value.map(|w| {
                    value_json.insert("Uint32".to_string(), JsonNum(w.into()));
                    w
                });
            }
            Witness::MerklePath(value) => {
                let mut path = Vec::new();
                value.map(|w| {
                    for node in w {
                        path.push(JsonStr(node.inner().to_string()));
                    }
                    w
                });
                value_json.insert("MerklePath".to_string(), JsonArray(path));
            }
            Witness::SparseMerklePath(value) => {
                let mut path = Vec::new();
                value.map(|w| {
                    for node in w {
                        path.push(JsonStr(node.to_string()));
                    }
                    w
                });
                value_json.insert("SparseMerklePath".to_string(), JsonArray(path));
            }
            Witness::EcNiPoint(value) => {
                let (mut x, mut y) = (pallas::Base::ZERO, pallas::Base::ZERO);
                value.map(|w| {
                    let coords = w.to_affine().coordinates().unwrap();
                    (x, y) = (*coords.x(), *coords.y());
                    w
                });
                let coords = vec![JsonStr(x.to_string()), JsonStr(y.to_string())];
                value_json.insert("EcNiPoint".to_string(), JsonArray(coords));
            }
            _ => unimplemented!(),
        }
        witnesses.push(JsonObj(value_json));
    }

    let mut instances = Vec::new();
    for instance in public_inputs {
        instances.push(JsonStr(instance.to_string()));
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

#[cfg(feature = "tinyjson")]
/// Import witness.json which can be used to debug or benchmark circuits.
/// Note that if the path or provided json is incorrect then this function will panic.
pub fn import_witness_json<P: AsRef<Path>>(input_path: P) -> (Vec<Witness>, Vec<pallas::Base>) {
    let mut input = File::open(input_path).expect("could not open input file");
    let mut json_str = String::new();
    input.read_to_string(&mut json_str).expect("unable to read to string");
    let json: JsonValue = json_str.parse().unwrap();
    drop(input);
    drop(json_str);

    let root: &HashMap<_, _> = json.get().expect("root");
    let json_witness: &Vec<_> = root["witnesses"].get().expect("witnesses");

    let jval_as_fp = |j_val: &JsonValue| {
        let valstr: &String = j_val.get().expect("value str");
        pallas::Base::from_str(valstr).unwrap()
    };

    let jval_as_vecfp = |j_val: &JsonValue| {
        j_val
            .get::<Vec<_>>()
            .expect("value str")
            .iter()
            .map(jval_as_fp)
            .collect::<Vec<pallas::Base>>()
    };

    let mut witnesses = Vec::new();
    for j_witness in json_witness {
        let item: &HashMap<_, _> = j_witness.get().expect("root");
        assert_eq!(item.len(), 1);
        let (typename, j_val) = item.iter().next().expect("witness has single item");
        match typename.as_str() {
            "Base" => {
                let fp = jval_as_fp(j_val);
                witnesses.push(Witness::Base(Value::known(fp)));
            }
            "Scalar" => {
                let valstr: &String = j_val.get().expect("value str");
                let fq = pallas::Scalar::from_str(valstr).unwrap();
                witnesses.push(Witness::Scalar(Value::known(fq)));
            }
            "Uint32" => {
                let val: &f64 = j_val.get().expect("value str");
                witnesses.push(Witness::Uint32(Value::known(*val as u32)));
            }
            "MerklePath" => {
                let vals: Vec<_> = jval_as_vecfp(j_val).into_iter().map(MerkleNode::new).collect();
                assert_eq!(vals.len(), 32);
                let vals: [MerkleNode; 32] = vals.try_into().unwrap();
                witnesses.push(Witness::MerklePath(Value::known(vals)));
            }
            "SparseMerklePath" => {
                let vals = jval_as_vecfp(j_val);
                assert_eq!(vals.len(), 255);
                let vals: [pallas::Base; 255] = vals.try_into().unwrap();
                witnesses.push(Witness::SparseMerklePath(Value::known(vals)));
            }
            "EcNiPoint" => {
                let vals = jval_as_vecfp(j_val);
                assert_eq!(vals.len(), 2);
                let (x, y) = (vals[0], vals[1]);
                let point: pallas::Point = pallas::Affine::from_xy(x, y).unwrap().to_curve();
                witnesses.push(Witness::EcNiPoint(Value::known(point)));
            }
            _ => unimplemented!(),
        }
    }

    let instances = jval_as_vecfp(&root["instances"]);

    (witnesses, instances)
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
            Witness::SparseMerklePath(_) => *binary_witness == zkas::VarType::SparseMerklePath,
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
