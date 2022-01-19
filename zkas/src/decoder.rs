use darkfi::{
    util::serial::{deserialize_partial, VarInt},
    Result,
};

use crate::{compiler::MAGIC_BYTES, opcode::Opcode, types::Type};

#[derive(Debug)]
pub struct ZkBinary {
    pub constants: Vec<(Type, String)>,
    pub witnesses: Vec<Type>,
    pub opcodes: Vec<(Opcode, u64, Vec<u64>)>,
}

impl ZkBinary {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let magic_bytes = &bytes[0..4];
        if magic_bytes != MAGIC_BYTES {
            panic!()
        }

        let _binary_version = &bytes[4];

        let constants_offset = match find_subslice(bytes, b".constant") {
            Some(v) => v,
            None => panic!(),
        };

        let contract_offset = match find_subslice(bytes, b".contract") {
            Some(v) => v,
            None => panic!(),
        };

        let circuit_offset = match find_subslice(bytes, b".circuit") {
            Some(v) => v,
            None => panic!(),
        };

        let debug_offset = match find_subslice(bytes, b".debug") {
            Some(v) => v,
            None => bytes.len(),
        };

        assert!(constants_offset < contract_offset);
        assert!(contract_offset < circuit_offset);
        assert!(circuit_offset < debug_offset);

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

    fn parse_circuit(_bytes: &[u8]) -> Result<Vec<(Opcode, u64, Vec<u64>)>> {
        Ok(vec![])
    }
}

// https://stackoverflow.com/questions/35901547/how-can-i-find-a-subsequence-in-a-u8-slice
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}
