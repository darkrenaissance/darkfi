use super::types::Type;

/// Opcodes supported by the VM
#[derive(Copy, Clone, Debug)]
#[repr(u8)]
pub enum Opcode {
    /// Elliptic curve addition
    EcAdd = 0x00,

    /// Elliptic curve multiplication
    EcMul = 0x01,

    /// Elliptic curve multiplication with a Base field element
    EcMulBase = 0x02,

    /// Elliptic curve multiplication with a u64 wrapped in a Scalar element
    EcMulShort = 0x03,

    /// Get the x coordinate of an elliptic curve point
    EcGetX = 0x08,

    /// Get the y coordinate of an elliptic curve point
    EcGetY = 0x09,

    /// Poseidon hash of N elements
    PoseidonHash = 0x10,

    /// Calculate merkle root given given a position, Merkle path, and an element
    CalculateMerkleRoot = 0x20,

    /// Base field element addition
    BaseAdd = 0x30,

    /// Base field element multiplication
    BaseMul = 0x31,

    /// Constrain a Base field element to a circuit's public input
    ConstrainInstance = 0xf0,

    /// Intermediate opcode for the compiler, should never appear in the result
    Noop = 0xff,
}

impl Opcode {
    /// Return a tuple of vectors of types that are accepted by a specific opcode
    /// `r.0` is the return type(s) and `r.1` is the argument type(s).
    pub fn arg_types(&self) -> (Vec<Type>, Vec<Type>) {
        match self {
            // (return_type, opcode_arg_types)
            Opcode::EcAdd => (vec![Type::EcPoint], vec![Type::EcPoint, Type::EcPoint]),
            Opcode::EcMul => (vec![Type::EcPoint], vec![Type::Scalar, Type::EcFixedPoint]),
            Opcode::EcMulBase => (vec![Type::EcPoint], vec![Type::Base, Type::EcFixedPointBase]),
            Opcode::EcMulShort => (vec![Type::EcPoint], vec![Type::Base, Type::EcFixedPointShort]),
            Opcode::EcGetX => (vec![Type::Base], vec![Type::EcPoint]),
            Opcode::EcGetY => (vec![Type::Base], vec![Type::EcPoint]),
            Opcode::PoseidonHash => (vec![Type::Base], vec![Type::BaseArray]),
            Opcode::CalculateMerkleRoot => {
                (vec![Type::Base], vec![Type::Uint32, Type::MerklePath, Type::Base])
            }
            Opcode::BaseAdd => (vec![Type::Base], vec![Type::Base, Type::Base]),
            Opcode::BaseMul => (vec![Type::Base], vec![Type::Base, Type::Base]),
            Opcode::ConstrainInstance => (vec![], vec![Type::Base]),
            Opcode::Noop => (vec![], vec![]),
        }
    }

    pub fn from_repr(b: u8) -> Self {
        match b {
            0x00 => Self::EcAdd,
            0x01 => Self::EcMul,
            0x02 => Self::EcMulBase,
            0x03 => Self::EcMulShort,
            0x08 => Self::EcGetX,
            0x09 => Self::EcGetY,
            0x10 => Self::PoseidonHash,
            0x20 => Self::CalculateMerkleRoot,
            0x30 => Self::BaseAdd,
            0x31 => Self::BaseMul,
            0xf0 => Self::ConstrainInstance,
            _ => unimplemented!(),
        }
    }
}
