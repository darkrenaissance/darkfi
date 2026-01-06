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

use darkfi_serial::{deserialize_limited_partial, deserialize_partial, VarInt};

use super::{
    compiler::MAGIC_BYTES,
    constants::{
        MAX_ARGS_PER_OPCODE, MAX_BIN_SIZE, MAX_CONSTANTS, MAX_HEAP_SIZE, MAX_K, MAX_LITERALS,
        MAX_NS_LEN, MAX_OPCODES, MAX_STRING_LEN, MAX_WITNESSES, MIN_BIN_SIZE, SECTION_CIRCUIT,
        SECTION_CONSTANT, SECTION_DEBUG, SECTION_LITERAL, SECTION_WITNESS,
    },
    types::HeapType,
    LitType, Opcode, VarType,
};
use crate::{Error::ZkasDecoderError as ZkasErr, Result};

/// A ZkBinary decoded from compiled zkas code.
/// This is used by the zkvm.
///
/// The binary format consists of:
/// - Header: magic bytes, version, k param, namespace
/// - `.constant` section: constant types and names
/// - `.literal` section: literal types and values
/// - `.witness` section: witness types
/// - `.circuit` section: opcoddes and their arguments
/// - `.debug` section (optional): debug informatioon
#[derive(Clone, Debug)]
// ANCHOR: zkbinary-struct
pub struct ZkBinary {
    pub namespace: String,
    pub k: u32,
    pub constants: Vec<(VarType, String)>,
    pub literals: Vec<(LitType, String)>,
    pub witnesses: Vec<VarType>,
    pub opcodes: Vec<(Opcode, Vec<(HeapType, usize)>)>,
    pub debug_info: Option<DebugInfo>,
}
// ANCHOR_END: zkbinary-struct

/// Debug information decoded from the optional .debug section
/// Contains source mappings to help debug circuit failures.
#[derive(Clone, Debug, Default)]
pub struct DebugInfo {
    /// Source locations (line, col) for each opcode
    pub opcode_locations: Vec<(usize, usize)>,
    /// Variable names for each heap entry (constants, witnesses, assigned vars in order)
    pub heap_names: Vec<String>,
    /// Literal values as strings
    pub literal_names: Vec<String>,
}

// https://stackoverflow.com/questions/35901547/how-can-i-find-a-subsequence-in-a-u8-slice
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn find_section(bytes: &[u8], section: &[u8]) -> Result<usize> {
    find_subslice(bytes, section).ok_or_else(|| {
        ZkasErr(format!("Could not find {} section", String::from_utf8_lossy(section)))
    })
}

/// Validate that a count is within limits and reasonable for the remaining bytes
fn validate_count(
    count: u64,
    max: usize,
    remaining_bytes: usize,
    item_name: &str,
) -> Result<usize> {
    let count = count as usize;

    if count > max {
        return Err(ZkasErr(format!(
            "{} count {} exceeds maximum allowed {}",
            item_name, count, max
        )));
    }

    // Sanity check: each item needs at least 1 byte
    if count > remaining_bytes {
        return Err(ZkasErr(format!(
            "{} count {} exceeds remaining bytes {}",
            item_name, count, remaining_bytes
        )));
    }

    Ok(count)
}

struct SectionOffsets {
    constant: usize,
    literal: usize,
    witness: usize,
    circuit: usize,
    debug: usize,
}

impl SectionOffsets {
    /// Find all section offsets in the binary and validate their order
    fn find(bytes: &[u8]) -> Result<Self> {
        let constant = find_section(bytes, SECTION_CONSTANT)?;
        let literal = find_section(bytes, SECTION_LITERAL)?;
        let witness = find_section(bytes, SECTION_WITNESS)?;
        let circuit = find_section(bytes, SECTION_CIRCUIT)?;
        // Debug section is optional, so use end of bytes if not present
        let debug = find_subslice(bytes, SECTION_DEBUG).unwrap_or(bytes.len());

        // Validate section order
        let sections = [
            (constant, ".constant"),
            (literal, ".literal"),
            (witness, ".witness"),
            (circuit, ".circuit"),
            (debug, "debug/EOF"),
        ];

        for i in 0..sections.len() - 1 {
            if sections[i].0 > sections[i + 1].0 {
                return Err(ZkasErr(format!(
                    "{} section appeared before {}",
                    sections[i + 1].1,
                    sections[i].1
                )));
            }
        }

        Ok(Self { constant, literal, witness, circuit, debug })
    }

    /// Extract the bytes for the constant section
    fn constant_bytes<'a>(&self, bytes: &'a [u8]) -> &'a [u8] {
        &bytes[self.constant + SECTION_CONSTANT.len()..self.literal]
    }

    /// Extract the bytes for the literal section
    fn literal_bytes<'a>(&self, bytes: &'a [u8]) -> &'a [u8] {
        &bytes[self.literal + SECTION_LITERAL.len()..self.witness]
    }

    /// Extract the bytes for the witness section
    fn witness_bytes<'a>(&self, bytes: &'a [u8]) -> &'a [u8] {
        &bytes[self.witness + SECTION_WITNESS.len()..self.circuit]
    }

    /// Extract the bytes for the circuit section
    fn circuit_bytes<'a>(&self, bytes: &'a [u8]) -> &'a [u8] {
        &bytes[self.circuit + SECTION_CIRCUIT.len()..self.debug]
    }

    /// Extract the bytes for the debug section if present
    fn debug_bytes<'a>(&self, bytes: &'a [u8]) -> Option<&'a [u8]> {
        if self.debug < bytes.len() {
            Some(&bytes[self.debug + SECTION_DEBUG.len()..])
        } else {
            None
        }
    }
}

impl ZkBinary {
    /// Decode a ZkBinary from compiled bytes
    pub fn decode(bytes: &[u8], decode_debug_symbols: bool) -> Result<Self> {
        // Ensure that bytes is a certain minimum length. Otherwise the code
        // below will panic due to an index out of bounds error.
        if bytes.len() < MIN_BIN_SIZE {
            return Err(ZkasErr("Not enough bytes".to_string()))
        }

        // Check max size to prevent decoding maliciously large binaries
        if bytes.len() > MAX_BIN_SIZE {
            return Err(ZkasErr(format!(
                "Binary size {} exceeds maximum allowed {}",
                bytes.len(),
                MAX_BIN_SIZE
            )))
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
            return Err(ZkasErr(format!("k param is too high, max allowed is {MAX_K}")))
        }

        // After the binary version and k, we're supposed to have the witness namespace
        let (namespace, _) = deserialize_limited_partial::<String>(&bytes[9..], MAX_NS_LEN)?;

        // ===============
        // Section parsing
        // ===============
        let offsets = SectionOffsets::find(bytes)?;

        let constants = Self::parse_constants(offsets.constant_bytes(bytes))?;
        let literals = Self::parse_literals(offsets.literal_bytes(bytes))?;
        let witnesses = Self::parse_witnesses(offsets.witness_bytes(bytes))?;
        let opcodes = Self::parse_circuit(offsets.circuit_bytes(bytes))?;

        let mut debug_info = None;
        if decode_debug_symbols {
            debug_info = match offsets.debug_bytes(bytes) {
                Some(debug_bytes) => Some(Self::parse_debug(debug_bytes)?),
                None => None,
            };
        }

        let binary = Self { namespace, k, constants, literals, witnesses, opcodes, debug_info };

        // Validate cross-references between sections
        binary.validate()?;

        Ok(binary)
    }

    /// Validate cross-references and consistency between sections.
    /// This catches malicious binaries that pass individual section
    /// parsing but have invalid references.
    fn validate(&self) -> Result<()> {
        // Calculate actual heap size: constants + witnesses + assigned vars
        // Each opcode that produces a result adds one entry to the heap
        let num_assignments = self
            .opcodes
            .iter()
            .filter(|(op, _)| {
                let (ret_types, _) = op.arg_types();
                !ret_types.is_empty()
            })
            .count();

        let heap_size = self.constants.len() + self.witnesses.len() + num_assignments;

        // Validate all heap references in opcodes
        for (op_idx, (opcode, args)) in self.opcodes.iter().enumerate() {
            // Calculate heap size at this point in execution
            // (constants + witnesses + results from previous opcodes)
            let prev_assignments = self.opcodes[..op_idx]
                .iter()
                .filter(|(op, _)| {
                    let (ret_types, _) = op.arg_types();
                    !ret_types.is_empty()
                })
                .count();
            let available_heap = self.constants.len() + self.witnesses.len() + prev_assignments;

            for (heap_type, heap_idx) in args {
                match heap_type {
                    HeapType::Var => {
                        if *heap_idx >= available_heap {
                            return Err(ZkasErr(format!(
                                "Opcode {} references heap idx {} but only {} entries available",
                                opcode.name(),
                                heap_idx,
                                available_heap
                            )));
                        }
                    }
                    HeapType::Lit => {
                        if *heap_idx >= self.literals.len() {
                            return Err(ZkasErr(format!(
                                "Opcode {} references literal idx {} but only {} literals exist",
                                opcode.name(),
                                heap_idx,
                                self.literals.len()
                            )));
                        }
                    }
                }
            }
        }
        // Validate debug info consistency if present
        if let Some(ref debug) = self.debug_info {
            if debug.opcode_locations.len() != self.opcodes.len() {
                return Err(ZkasErr(format!(
                    "Debug info has {} opcode locations but circuit has {} opcodes",
                    debug.opcode_locations.len(),
                    self.opcodes.len()
                )));
            }

            if debug.heap_names.len() != heap_size {
                return Err(ZkasErr(format!(
                    "Debug info has {} heap names but heap has {} entries",
                    debug.heap_names.len(),
                    heap_size
                )));
            }

            if debug.literal_names.len() != self.literals.len() {
                return Err(ZkasErr(format!(
                    "Debug info has {} literal names but {} literals exist",
                    debug.literal_names.len(),
                    self.literals.len()
                )));
            }
        }

        Ok(())
    }

    fn parse_constants(bytes: &[u8]) -> Result<Vec<(VarType, String)>> {
        let mut constants = vec![];
        let mut offset = 0;

        while offset < bytes.len() {
            // Check we haven't exceeded the limit
            if constants.len() >= MAX_CONSTANTS {
                return Err(ZkasErr(format!(
                    "Too many constants, maximum allowed is {MAX_CONSTANTS}"
                )))
            }

            let c_type = VarType::from_repr(bytes[offset]).ok_or_else(|| {
                ZkasErr(format!("Could not decode constant VarType from {}", bytes[offset]))
            })?;
            offset += 1;

            let (name, len) =
                deserialize_limited_partial::<String>(&bytes[offset..], MAX_STRING_LEN)?;
            offset += len;

            constants.push((c_type, name));
        }

        Ok(constants)
    }

    fn parse_literals(bytes: &[u8]) -> Result<Vec<(LitType, String)>> {
        let mut literals = vec![];
        let mut offset = 0;

        while offset < bytes.len() {
            // Check we haven't exceeded the limit
            if literals.len() >= MAX_LITERALS {
                return Err(ZkasErr(format!(
                    "Too many literals, maximum allowed is {MAX_LITERALS}"
                )));
            }

            let l_type = LitType::from_repr(bytes[offset]).ok_or_else(|| {
                ZkasErr(format!("Could not decode literal LitType from {}", bytes[offset]))
            })?;
            offset += 1;

            let (name, len) =
                deserialize_limited_partial::<String>(&bytes[offset..], MAX_STRING_LEN)?;
            offset += len;

            literals.push((l_type, name));
        }

        Ok(literals)
    }

    fn parse_witnesses(bytes: &[u8]) -> Result<Vec<VarType>> {
        // Check vount before allocating
        if bytes.len() > MAX_WITNESSES {
            return Err(ZkasErr(format!(
                "Too many witnesses ({}), maximum allowed is {}",
                bytes.len(),
                MAX_WITNESSES
            )));
        }

        let mut witnesses = Vec::with_capacity(bytes.len());

        for &byte in bytes {
            let w_type = VarType::from_repr(byte).ok_or_else(|| {
                ZkasErr(format!("Could not decode witness VarType from {}", byte))
            })?;

            witnesses.push(w_type);
        }

        Ok(witnesses)
    }

    #[allow(clippy::type_complexity)]
    fn parse_circuit(bytes: &[u8]) -> Result<Vec<(Opcode, Vec<(HeapType, usize)>)>> {
        let mut opcodes = vec![];
        let mut offset = 0;

        while offset < bytes.len() {
            // Check opcode count limit
            if opcodes.len() >= MAX_OPCODES {
                return Err(ZkasErr(format!("Too many opcodes, maximum allowed is {MAX_OPCODES}")))
            }

            let opcode = Opcode::from_repr(bytes[offset]).ok_or_else(|| {
                ZkasErr(format!("Could not decode Opcode from {}", bytes[offset]))
            })?;
            offset += 1;

            // TODO: Check that the types and arg number are correct

            // Parse argument count
            let (arg_count, len) = deserialize_partial::<VarInt>(&bytes[offset..])?;
            offset += len;

            // Validate argument count
            let arg_count =
                validate_count(arg_count.0, MAX_ARGS_PER_OPCODE, bytes.len() - offset, "Argument")?;

            // Parse arguments
            let mut args = Vec::with_capacity(arg_count);
            for _ in 0..arg_count {
                // Check bounds to prevent panics
                if offset >= bytes.len() {
                    return Err(ZkasErr(format!(
                        "Bad offset for circuit: offset {} is >= circuit len {}",
                        offset,
                        bytes.len()
                    )));
                }

                let heap_type_byte = bytes[offset];
                offset += 1;

                if offset >= bytes.len() {
                    return Err(ZkasErr(format!(
                        "Bad offset for circuit: offset {} is >= circuit len {}",
                        offset,
                        bytes.len()
                    )));
                }

                let (heap_index, len) = deserialize_partial::<VarInt>(&bytes[offset..])?;
                offset += len;

                let heap_type = HeapType::from_repr(heap_type_byte).ok_or_else(|| {
                    ZkasErr(format!("Could not decode HeapType from {}", heap_type_byte))
                })?;

                // Validate heap index is reasonable
                let heap_idx = heap_index.0 as usize;
                if heap_idx > MAX_HEAP_SIZE {
                    return Err(ZkasErr(format!(
                        "Heap index {} exceeds maximum allowed {}",
                        heap_idx, MAX_HEAP_SIZE
                    )));
                }

                args.push((heap_type, heap_index.0 as usize));
            }

            opcodes.push((opcode, args));
        }

        Ok(opcodes)
    }

    fn parse_debug(bytes: &[u8]) -> Result<DebugInfo> {
        let mut offset = 0;

        // Parse opcode source locations
        let (num_opcodes, len) = deserialize_partial::<VarInt>(&bytes[offset..])?;
        offset += len;

        let num_opcodes =
            validate_count(num_opcodes.0, MAX_OPCODES, bytes.len() - offset, "Debug opcode")?;

        let mut opcode_locations = Vec::with_capacity(num_opcodes);
        for _ in 0..num_opcodes {
            let (line, len) = deserialize_partial::<VarInt>(&bytes[offset..])?;
            offset += len;
            let (column, len) = deserialize_partial::<VarInt>(&bytes[offset..])?;
            offset += len;
            opcode_locations.push((line.0 as usize, column.0 as usize));
        }

        // Parse heap var names
        let (heap_size, len) = deserialize_partial::<VarInt>(&bytes[offset..])?;
        offset += len;

        let heap_size =
            validate_count(heap_size.0, MAX_HEAP_SIZE, bytes.len() - offset, "Debug heap")?;

        let mut heap_names = Vec::with_capacity(heap_size);
        for _ in 0..heap_size {
            let (name, len) =
                deserialize_limited_partial::<String>(&bytes[offset..], MAX_STRING_LEN)?;
            offset += len;
            heap_names.push(name);
        }

        // Parse literal names
        let (num_literals, len) = deserialize_partial::<VarInt>(&bytes[offset..])?;
        offset += len;

        let num_literals =
            validate_count(num_literals.0, MAX_LITERALS, bytes.len() - offset, "Debug literal")?;

        let mut literal_names = Vec::with_capacity(num_literals);
        for _ in 0..num_literals {
            let (name, len) =
                deserialize_limited_partial::<String>(&bytes[offset..], MAX_STRING_LEN)?;
            offset += len;
            literal_names.push(name);
        }

        Ok(DebugInfo { opcode_locations, heap_names, literal_names })
    }

    /// Get the source location (line, column) for a given opcode index.
    /// Returns `None` if debug info is not present or index is OOB.
    pub fn opcode_location(&self, opcode_idx: usize) -> Option<(usize, usize)> {
        self.debug_info.as_ref()?.opcode_locations.get(opcode_idx).copied()
    }

    /// Get the variable name for a given heap index.
    /// Returns `None` if debug info is not present or index is OOB.
    pub fn heap_name(&self, heap_idx: usize) -> Option<&str> {
        self.debug_info.as_ref()?.heap_names.get(heap_idx).map(|s| s.as_str())
    }

    /// Get the literal name/value for a given literal index.
    /// Returns `None` if debug info is not present or index is OOB.
    pub fn literal_name(&self, literal_idx: usize) -> Option<&str> {
        self.debug_info.as_ref()?.literal_names.get(literal_idx).map(|s| s.as_str())
    }

    /// Check if debug info is present
    pub fn has_debug_info(&self) -> bool {
        self.debug_info.is_some()
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
        let _dec = ZkBinary::decode(&data, true);
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
        let _dec = ZkBinary::decode(&data, true);
    }
}
