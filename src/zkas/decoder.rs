use super::{compiler::MAGIC_BYTES, types::StackType, LitType, Opcode, VarType};
use crate::{
    serial::{deserialize_partial, VarInt},
    Error::ZkasDecoderError as ZkasErr,
    Result,
};

/// A ZkBinary decoded from compiled zkas code.
/// This is used by the zkvm.
#[derive(Clone, Debug)]
pub struct ZkBinary {
    pub constants: Vec<(VarType, String)>,
    pub literals: Vec<(LitType, String)>,
    pub witnesses: Vec<VarType>,
    pub opcodes: Vec<(Opcode, Vec<(StackType, usize)>)>,
}

// https://stackoverflow.com/questions/35901547/how-can-i-find-a-subsequence-in-a-u8-slice
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

impl ZkBinary {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let magic_bytes = &bytes[0..4];
        if magic_bytes != MAGIC_BYTES {
            return Err(ZkasErr("Magic bytes are incorrect.".to_string()))
        }

        let _binary_version = &bytes[4];

        let constants_offset = match find_subslice(bytes, b".constant") {
            Some(v) => v,
            None => return Err(ZkasErr("Could not find .constant section".to_string())),
        };

        let literals_offset = match find_subslice(bytes, b".literal") {
            Some(v) => v,
            None => return Err(ZkasErr("Could not find .literal section".to_string())),
        };

        let contract_offset = match find_subslice(bytes, b".contract") {
            Some(v) => v,
            None => return Err(ZkasErr("Could not find .contract section".to_string())),
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

        if literals_offset > contract_offset {
            return Err(ZkasErr(".contract section appeared before .literal".to_string()))
        }

        if contract_offset > circuit_offset {
            return Err(ZkasErr(".circuit section appeared before .contract".to_string()))
        }

        if circuit_offset > debug_offset {
            return Err(ZkasErr(".debug section appeared before .circuit or EOF".to_string()))
        }

        let constants_section = &bytes[constants_offset + b".constant".len()..literals_offset];
        let literals_section = &bytes[literals_offset + b".literal".len()..contract_offset];
        let contract_section = &bytes[contract_offset + b".contract".len()..circuit_offset];
        let circuit_section = &bytes[circuit_offset + b".circuit".len()..debug_offset];

        let constants = ZkBinary::parse_constants(constants_section)?;
        let literals = ZkBinary::parse_literals(literals_section)?;
        let witnesses = ZkBinary::parse_contract(contract_section)?;
        let opcodes = ZkBinary::parse_circuit(circuit_section)?;
        // TODO: Debug info

        Ok(Self { constants, literals, witnesses, opcodes })
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

    fn parse_contract(bytes: &[u8]) -> Result<Vec<VarType>> {
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
    fn parse_circuit(bytes: &[u8]) -> Result<Vec<(Opcode, Vec<(StackType, usize)>)>> {
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

            let (arg_num, offset) = deserialize_partial::<VarInt>(&bytes[iter_offset..])?;
            iter_offset += offset;

            let mut args = vec![];
            for _ in 0..arg_num.0 {
                let stack_type = bytes[iter_offset];
                iter_offset += 1;
                let (stack_index, offset) = deserialize_partial::<VarInt>(&bytes[iter_offset..])?;
                iter_offset += offset;
                let stack_type = match StackType::from_repr(stack_type) {
                    Some(v) => v,
                    None => {
                        return Err(ZkasErr(format!(
                            "Could not decode StackType from {}",
                            stack_type
                        )))
                    }
                };
                args.push((stack_type, stack_index.0 as usize)); // FIXME, why?
            }

            opcodes.push((opcode, args));
        }

        Ok(opcodes)
    }
}
