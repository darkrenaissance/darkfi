use darkfi::{
    util::serial::{deserialize_partial, VarInt},
    Error::ZkasDecoderError,
    Result,
};

use crate::{compiler::MAGIC_BYTES, opcode::Opcode, types::Type};

#[derive(Debug)]
pub struct ZkBinary {
    pub constants: Vec<(Type, String)>,
    pub witnesses: Vec<Type>,
    pub opcodes: Vec<(Opcode, Vec<u64>)>,
}

impl ZkBinary {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let magic_bytes = &bytes[0..4];
        if magic_bytes != MAGIC_BYTES {
            return Err(ZkasDecoderError("Magic bytes are incorrect."))
        }

        let _binary_version = &bytes[4];

        let constants_offset = match find_subslice(bytes, b".constant") {
            Some(v) => v,
            None => return Err(ZkasDecoderError("Could not find .constant section.")),
        };

        let contract_offset = match find_subslice(bytes, b".contract") {
            Some(v) => v,
            None => return Err(ZkasDecoderError("Could not find .contract section")),
        };

        let circuit_offset = match find_subslice(bytes, b".circuit") {
            Some(v) => v,
            None => return Err(ZkasDecoderError("Could not find .circuit section")),
        };

        let debug_offset = match find_subslice(bytes, b".debug") {
            Some(v) => v,
            None => bytes.len(),
        };

        if constants_offset < contract_offset {
            return Err(ZkasDecoderError(".contract appeared before .constant"))
        }

        if contract_offset < circuit_offset {
            return Err(ZkasDecoderError(".contract appeared before .circuit"))
        }

        if circuit_offset < debug_offset {
            return Err(ZkasDecoderError(".circuit appeared before .debug or EOF"))
        }

        let constants_section = &bytes[constants_offset + b".constant".len()..contract_offset];
        let contract_section = &bytes[contract_offset + b".contract".len()..circuit_offset];
        let circuit_section = &bytes[circuit_offset + b".circuit".len()..debug_offset];

        let constants = ZkBinary::parse_constants(constants_section)?;
        let witnesses = ZkBinary::parse_contract(contract_section)?;
        let opcodes = ZkBinary::parse_circuit(circuit_section)?;
        // TODO: Debug info

        Ok(Self { constants, witnesses, opcodes })
    }

    fn parse_constants(bytes: &[u8]) -> Result<Vec<(Type, String)>> {
        let mut constants = vec![];

        let mut iter_offset = 0;
        while iter_offset < bytes.len() {
            let c_type = Type::from_repr(bytes[iter_offset]);
            iter_offset += 1;
            let (name, offset) = deserialize_partial::<String>(&bytes[iter_offset..])?;
            iter_offset += offset;

            constants.push((c_type, name));
        }

        Ok(constants)
    }

    fn parse_contract(bytes: &[u8]) -> Result<Vec<Type>> {
        let mut witnesses = vec![];

        let mut iter_offset = 0;
        while iter_offset < bytes.len() {
            let w_type = Type::from_repr(bytes[iter_offset]);
            iter_offset += 1;

            witnesses.push(w_type);
        }

        Ok(witnesses)
    }

    fn parse_circuit(bytes: &[u8]) -> Result<Vec<(Opcode, Vec<u64>)>> {
        let mut opcodes = vec![];

        let mut iter_offset = 0;
        while iter_offset < bytes.len() {
            let opcode = Opcode::from_repr(bytes[iter_offset]);
            iter_offset += 1;

            let (arg_num, offset) = deserialize_partial::<VarInt>(&bytes[iter_offset..])?;
            iter_offset += offset;

            let mut args = vec![];
            for _ in 0..arg_num.0 {
                let (stack_index, offset) = deserialize_partial::<VarInt>(&bytes[iter_offset..])?;
                iter_offset += offset;
                args.push(stack_index.0);
            }

            opcodes.push((opcode, args));
        }

        Ok(opcodes)
    }
}

// https://stackoverflow.com/questions/35901547/how-can-i-find-a-subsequence-in-a-u8-slice
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}
