/// Stack types in bincode & vm
#[derive(Clone, Debug)]
#[repr(u8)]
pub enum StackType {
    Var = 0x00,
    Lit = 0x01,
}

impl StackType {
    pub fn from_repr(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::Var),
            0x01 => Some(Self::Lit),
            _ => None,
        }
    }
}

/// Varable types supported by the zkas VM
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum VarType {
    /// Dummy intermediate type
    Dummy = 0x00,

    /// Elliptic curve point
    EcPoint = 0x01,

    /// Elliptic curve fixed point (a constant)
    EcFixedPoint = 0x02,

    /// Elliptic curve fixed point short
    EcFixedPointShort = 0x03,

    /// Elliptic curve fixed point in base field
    EcFixedPointBase = 0x04,

    /// Base field element
    Base = 0x10,

    /// Base field element array
    BaseArray = 0x11,

    /// Scalar field element
    Scalar = 0x12,

    /// Scalar field element array
    ScalarArray = 0x13,

    /// A Merkle tree path
    MerklePath = 0x20,

    /// Unsigned 32-bit integer
    Uint32 = 0x30,

    /// Unsigned 64-bit integer
    Uint64 = 0x31,
}

impl VarType {
    pub fn from_repr(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(Self::EcPoint),
            0x02 => Some(Self::EcFixedPoint),
            0x03 => Some(Self::EcFixedPointShort),
            0x04 => Some(Self::EcFixedPointBase),
            0x10 => Some(Self::Base),
            0x11 => Some(Self::BaseArray),
            0x12 => Some(Self::Scalar),
            0x13 => Some(Self::ScalarArray),
            0x20 => Some(Self::MerklePath),
            0x30 => Some(Self::Uint32),
            0x31 => Some(Self::Uint64),
            _ => None,
        }
    }
}

/// Literal types supported by the zkas VM
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum LitType {
    /// Dummy intermediate type
    Dummy = 0x00,

    /// Unsigned 64-bit integer
    Uint64 = 0x01,
}

impl LitType {
    pub fn from_repr(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(Self::Uint64),
            _ => None,
        }
    }

    pub fn to_vartype(&self) -> VarType {
        match self {
            Self::Dummy => VarType::Dummy,
            Self::Uint64 => VarType::Uint64,
        }
    }
}
