use super::VarType;

/// Opcodes supported by the zkas VM
#[derive(Copy, Clone, Debug)]
#[repr(u8)]
pub enum Opcode {
    /// Intermediate opcode for the compiler, should never appear in the result
    Noop = 0x00,

    /// Elliptic curve addition
    EcAdd = 0x01,

    // Elliptic curve multiplication
    EcMul = 0x02,

    /// Elliptic curve multiplication with a Base field element
    EcMulBase = 0x03,

    /// Elliptic curve multiplication with a Base field element of 64bit width
    EcMulShort = 0x04,

    /// Get the x coordinate of an elliptic curve point
    EcGetX = 0x08,

    /// Get the y coordinate of an elliptic curve point
    EcGetY = 0x09,

    /// Poseidon hash of N Base field elements
    PoseidonHash = 0x10,

    /// Calculate Merkle root, given a position, Merkle path, and an element
    MerkleRoot = 0x20,

    /// Base field element addition
    BaseAdd = 0x30,

    /// Base field element multiplication
    BaseMul = 0x31,

    /// Base field element subtraction
    BaseSub = 0x32,

    /// Witness an unsigned integer into a Base field element
    WitnessBase = 0x40,

    /// Range check a Base field element, given bit-width (up to 253)
    RangeCheck = 0x50,

    /// Compare two Base field elements and see if a is less than b
    LessThan = 0x51,

    /// Check if a field element fits in a boolean (Either 0 or 1)
    BoolCheck = 0x52,

    /// Constrain a Base field element to a circuit's public input
    ConstrainInstance = 0xf0,

    /// Debug a variable's value in the ZK circuit table.
    DebugPrint = 0xff,
}

impl Opcode {
    pub fn from_name(n: &str) -> Option<Self> {
        match n {
            "ec_add" => Some(Self::EcAdd),
            "ec_mul" => Some(Self::EcMul),
            "ec_mul_base" => Some(Self::EcMulBase),
            "ec_mul_short" => Some(Self::EcMulShort),
            "ec_get_x" => Some(Self::EcGetX),
            "ec_get_y" => Some(Self::EcGetY),
            "poseidon_hash" => Some(Self::PoseidonHash),
            "merkle_root" => Some(Self::MerkleRoot),
            "base_add" => Some(Self::BaseAdd),
            "base_mul" => Some(Self::BaseMul),
            "base_sub" => Some(Self::BaseSub),
            "witness_base" => Some(Self::WitnessBase),
            "range_check" => Some(Self::RangeCheck),
            "less_than" => Some(Self::LessThan),
            "bool_check" => Some(Self::BoolCheck),
            "constrain_instance" => Some(Self::ConstrainInstance),
            "debug" => Some(Self::DebugPrint),
            _ => None,
        }
    }

    pub fn from_repr(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(Self::EcAdd),
            0x02 => Some(Self::EcMul),
            0x03 => Some(Self::EcMulBase),
            0x04 => Some(Self::EcMulShort),
            0x08 => Some(Self::EcGetX),
            0x09 => Some(Self::EcGetY),
            0x10 => Some(Self::PoseidonHash),
            0x20 => Some(Self::MerkleRoot),
            0x30 => Some(Self::BaseAdd),
            0x31 => Some(Self::BaseMul),
            0x32 => Some(Self::BaseSub),
            0x40 => Some(Self::WitnessBase),
            0x50 => Some(Self::RangeCheck),
            0x51 => Some(Self::LessThan),
            0x52 => Some(Self::BoolCheck),
            0xf0 => Some(Self::ConstrainInstance),
            0xff => Some(Self::DebugPrint),
            _ => None,
        }
    }

    /// Return a tuple of vectors of types that are accepted by a specific opcode.
    /// `r.0` is the return type(s), and `r.1` is the argument type(s).
    pub fn arg_types(&self) -> (Vec<VarType>, Vec<VarType>) {
        match self {
            Opcode::Noop => (vec![], vec![]),

            Opcode::EcAdd => (vec![VarType::EcPoint], vec![VarType::EcPoint, VarType::EcPoint]),

            Opcode::EcMul => (vec![VarType::EcPoint], vec![VarType::Scalar, VarType::EcFixedPoint]),

            Opcode::EcMulBase => {
                (vec![VarType::EcPoint], vec![VarType::Base, VarType::EcFixedPointBase])
            }

            Opcode::EcMulShort => {
                (vec![VarType::EcPoint], vec![VarType::Base, VarType::EcFixedPointShort])
            }

            Opcode::EcGetX => (vec![VarType::Base], vec![VarType::EcPoint]),

            Opcode::EcGetY => (vec![VarType::Base], vec![VarType::EcPoint]),

            Opcode::PoseidonHash => (vec![VarType::Base], vec![VarType::BaseArray]),

            Opcode::MerkleRoot => {
                (vec![VarType::Base], vec![VarType::Uint32, VarType::MerklePath, VarType::Base])
            }

            Opcode::BaseAdd => (vec![VarType::Base], vec![VarType::Base, VarType::Base]),

            Opcode::BaseMul => (vec![VarType::Base], vec![VarType::Base, VarType::Base]),

            Opcode::BaseSub => (vec![VarType::Base], vec![VarType::Base, VarType::Base]),

            Opcode::WitnessBase => (vec![VarType::Base], vec![VarType::Uint64]),

            Opcode::RangeCheck => (vec![], vec![VarType::Uint64, VarType::Base]),

            Opcode::LessThan => (vec![], vec![VarType::Base, VarType::Base]),

            Opcode::BoolCheck => (vec![], vec![VarType::Base]),

            Opcode::ConstrainInstance => (vec![], vec![VarType::Base]),

            Opcode::DebugPrint => (vec![], vec![]),
        }
    }
}
