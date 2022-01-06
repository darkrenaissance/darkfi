use crate::types::Type;

/// Opcodes supported by the VM
#[derive(Copy, Clone, Debug)]
#[repr(u8)]
pub enum Opcode {
    EcAdd = 0x00,
    EcMul = 0x01,
    EcMulBase = 0x02,
    EcMulShort = 0x03,

    EcGetX = 0x08,
    EcGetY = 0x09,

    PoseidonHash = 0x10,

    CalculateMerkleRoot = 0x20,

    ConstrainInstance = 0xf0,

    Noop = 0xff,
}

impl Opcode {
    pub fn arg_types(&self) -> (Vec<Type>, Vec<Type>) {
        match self {
            // (return_type, opcode_arg_types)
            Opcode::EcAdd => (vec![Type::EcPoint], vec![Type::EcPoint, Type::EcPoint]),
            Opcode::EcMul => (vec![Type::EcPoint], vec![Type::Scalar, Type::EcFixedPoint]),
            Opcode::EcMulBase => (vec![Type::EcPoint], vec![Type::Base, Type::EcFixedPoint]),
            Opcode::EcMulShort => (vec![Type::EcPoint], vec![Type::Base, Type::EcFixedPoint]),
            Opcode::EcGetX => (vec![Type::Base], vec![Type::EcPoint]),
            Opcode::EcGetY => (vec![Type::Base], vec![Type::EcPoint]),
            Opcode::PoseidonHash => (vec![Type::Base], vec![Type::BaseArray]),
            Opcode::CalculateMerkleRoot => (vec![Type::Base], vec![Type::MerklePath, Type::Base]),
            Opcode::ConstrainInstance => (vec![], vec![Type::Base]),
            Opcode::Noop => (vec![], vec![]),
        }
    }
}
