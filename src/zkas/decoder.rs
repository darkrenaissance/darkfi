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

use darkfi_serial::{deserialize_partial, VarInt};

use super::{
    compiler::MAGIC_BYTES,
    constants::{MAX_K, MAX_NS_LEN, MIN_BIN_SIZE},
    types::HeapType,
    LitType, Opcode, VarType,
};
use crate::{Error::ZkasDecoderError as ZkasErr, Result};

/// A ZkBinary decoded from compiled zkas code.
/// This is used by the zkvm.
#[derive(Clone, Debug)]
// ANCHOR: zkbinary-struct
pub struct ZkBinary {
    pub namespace: String,
    pub k: u32,
    pub constants: Vec<(VarType, String)>,
    pub literals: Vec<(LitType, String)>,
    pub witnesses: Vec<VarType>,
    pub opcodes: Vec<(Opcode, Vec<(HeapType, usize)>)>,
}
// ANCHOR_END: zkbinary-struct

// https://stackoverflow.com/questions/35901547/how-can-i-find-a-subsequence-in-a-u8-slice
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

impl ZkBinary {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        // Ensure that bytes is a certain minimum length. Otherwise the code
        // below will panic due to an index out of bounds error.
        if bytes.len() < MIN_BIN_SIZE {
            return Err(ZkasErr("Not enough bytes".to_string()))
        }
        let magic_bytes = &bytes[0..4];
        if magic_bytes != MAGIC_BYTES {
            return Err(ZkasErr("Magic bytes are incorrect".to_string()))
        }

        let _binary_version = &bytes[4];

        // Deserialize the k param
        let (k, _): (u32, _) = deserialize_partial(&bytes[5..9])?;

        // For now, we'll limit k.
        if k > MAX_K {
            return Err(ZkasErr("k param is too high, max allowed is 16".to_string()))
        }

        // After the binary version and k, we're supposed to have the witness namespace
        let (namespace, _): (String, _) = deserialize_partial(&bytes[9..])?;

        // Enforce a limit on the namespace string length
        if namespace.len() > MAX_NS_LEN {
            return Err(ZkasErr("Namespace too long".to_string()))
        }

        let constants_offset = match find_subslice(bytes, b".constant") {
            Some(v) => v,
            None => return Err(ZkasErr("Could not find .constant section".to_string())),
        };

        let literals_offset = match find_subslice(bytes, b".literal") {
            Some(v) => v,
            None => return Err(ZkasErr("Could not find .literal section".to_string())),
        };

        let witness_offset = match find_subslice(bytes, b".witness") {
            Some(v) => v,
            None => return Err(ZkasErr("Could not find .witness section".to_string())),
        };

        let circuit_offset = match find_subslice(bytes, b".circuit") {
            Some(v) => v,
            None => return Err(ZkasErr("Could not find .circuit section".to_string())),
        };

        let debug_offset = match find_subslice(bytes, b".debug") {
            Some(v) => v,
            None => bytes.len(),
        };

        if constants_offset > literals_offset {
            return Err(ZkasErr(".literal section appeared before .constant".to_string()))
        }

        if literals_offset > witness_offset {
            return Err(ZkasErr(".witness section appeared before .literal".to_string()))
        }

        if witness_offset > circuit_offset {
            return Err(ZkasErr(".circuit section appeared before .witness".to_string()))
        }

        if circuit_offset > debug_offset {
            return Err(ZkasErr(".debug section appeared before .circuit or EOF".to_string()))
        }

        let constants_section = &bytes[constants_offset + b".constant".len()..literals_offset];
        let literals_section = &bytes[literals_offset + b".literal".len()..witness_offset];
        let witness_section = &bytes[witness_offset + b".witness".len()..circuit_offset];
        let circuit_section = &bytes[circuit_offset + b".circuit".len()..debug_offset];

        let constants = ZkBinary::parse_constants(constants_section)?;
        let literals = ZkBinary::parse_literals(literals_section)?;
        let witnesses = ZkBinary::parse_witness(witness_section)?;
        let opcodes = ZkBinary::parse_circuit(circuit_section)?;

        // TODO: Debug info

        Ok(Self { namespace, k, constants, literals, witnesses, opcodes })
    }

    fn parse_constants(bytes: &[u8]) -> Result<Vec<(VarType, String)>> {
        let mut constants = vec![];

        let mut iter_offset = 0;
        while iter_offset < bytes.len() {
            let c_type = match VarType::from_repr(bytes[iter_offset]) {
                Some(v) => v,
                None => {
                    return Err(ZkasErr(format!(
                        "Could not decode constant VarType from {}",
                        bytes[iter_offset],
                    )))
                }
            };
            iter_offset += 1;
            let (name, offset) = deserialize_partial::<String>(&bytes[iter_offset..])?;
            iter_offset += offset;

            constants.push((c_type, name));
        }

        Ok(constants)
    }

    fn parse_literals(bytes: &[u8]) -> Result<Vec<(LitType, String)>> {
        let mut literals = vec![];

        let mut iter_offset = 0;
        while iter_offset < bytes.len() {
            let l_type = match LitType::from_repr(bytes[iter_offset]) {
                Some(v) => v,
                None => {
                    return Err(ZkasErr(format!(
                        "Could not decode literal LitType from {}",
                        bytes[iter_offset],
                    )))
                }
            };
            iter_offset += 1;
            let (name, offset) = deserialize_partial::<String>(&bytes[iter_offset..])?;
            iter_offset += offset;

            literals.push((l_type, name));
        }

        Ok(literals)
    }

    fn parse_witness(bytes: &[u8]) -> Result<Vec<VarType>> {
        let mut witnesses = vec![];

        let mut iter_offset = 0;
        while iter_offset < bytes.len() {
            let w_type = match VarType::from_repr(bytes[iter_offset]) {
                Some(v) => v,
                None => {
                    return Err(ZkasErr(format!(
                        "Could not decode witness VarType from {}",
                        bytes[iter_offset],
                    )))
                }
            };

            iter_offset += 1;

            witnesses.push(w_type);
        }

        Ok(witnesses)
    }

    #[allow(clippy::type_complexity)]
    fn parse_circuit(bytes: &[u8]) -> Result<Vec<(Opcode, Vec<(HeapType, usize)>)>> {
        let mut opcodes = vec![];

        let mut iter_offset = 0;
        while iter_offset < bytes.len() {
            let opcode = match Opcode::from_repr(bytes[iter_offset]) {
                Some(v) => v,
                None => {
                    return Err(ZkasErr(format!(
                        "Could not decode Opcode from {}",
                        bytes[iter_offset]
                    )))
                }
            };
            iter_offset += 1;

            // TODO: Check that the types and arg number are correct

            let (arg_num, offset) = deserialize_partial::<VarInt>(&bytes[iter_offset..])?;
            iter_offset += offset;

            let mut args = vec![];
            for _ in 0..arg_num.0 {
                // Check bounds each time bytes[iter_offset] is accessed to prevent panics.
                if iter_offset >= bytes.len() {
                    return Err(ZkasErr(format!(
                        "Bad offset for circuit: offset {} is >= circuit length {}",
                        iter_offset,
                        bytes.len()
                    )))
                }
                let heap_type = bytes[iter_offset];
                iter_offset += 1;

                if iter_offset >= bytes.len() {
                    return Err(ZkasErr(format!(
                        "Bad offset for circuit: offset {} is >= circuit length {}",
                        iter_offset,
                        bytes.len()
                    )))
                }
                let (heap_index, offset) = deserialize_partial::<VarInt>(&bytes[iter_offset..])?;
                iter_offset += offset;
                let heap_type = match HeapType::from_repr(heap_type) {
                    Some(v) => v,
                    None => {
                        return Err(ZkasErr(format!("Could not decode HeapType from {heap_type}")))
                    }
                };
                args.push((heap_type, heap_index.0 as usize));
            }

            opcodes.push((opcode, args));
        }

        Ok(opcodes)
    }
}

#[cfg(test)]
mod tests {
    use crate::zkas::ZkBinary;

    #[test]
    fn panic_regression_001() {
        // Out-of-memory panic from string deserialization.
        // Read `doc/src/zkas/bincode.md` to understand the input.
        let data = vec![11u8, 1, 177, 53, 1, 0, 0, 0, 0, 255, 0, 204, 200, 72, 72, 72, 72, 1];
        let _dec = ZkBinary::decode(&data);
    }

    #[test]
    fn panic_regression_002() {
        // Index out of bounds panic in parse_circuit().
        // Read `doc/src/zkas/bincode.md` to understand the input.
        let data = vec![
            11u8, 1, 177, 53, 2, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 83, 105,
            109, 112, 108, 101, 46, 99, 111, 110, 115, 116, 97, 110, 116, 3, 18, 86, 65, 76, 85,
            69, 95, 67, 79, 77, 77, 73, 84, 95, 86, 65, 76, 85, 69, 2, 19, 86, 65, 76, 85, 69, 95,
            67, 79, 77, 77, 73, 84, 95, 82, 65, 77, 68, 79, 77, 46, 108, 105, 116, 101, 114, 97,
            108, 46, 119, 105, 116, 110, 101, 115, 115, 16, 18, 46, 99, 105, 114, 99, 117, 105,
            116, 4, 2, 0, 2, 0, 0, 2, 2, 0, 3, 0, 1, 8, 2, 0, 4, 0, 5, 8, 1, 0, 6, 9, 1, 0, 6, 240,
            1, 0, 7, 240, 41, 0, 0, 0, 1, 0, 8,
        ];
        let _dec = ZkBinary::decode(&data);
    }
}
