/// Types supported by the VM
#[derive(Copy, Clone, PartialEq, Debug)]
#[repr(u8)]
pub enum Type {
    /// Elliptic curve point
    EcPoint = 0x00,

    /// Elliptic curve fixed point (a constant)
    EcFixedPoint = 0x01,

    /// Elliptic curve fixed point short
    EcFixedPointShort = 0x02,

    /// Elliptic curve fixed point in base field
    EcFixedPointBase = 0x03,

    /// Base field element
    Base = 0x10,

    /// Array of Base field elements
    BaseArray = 0x11,

    /// Scalar field element
    Scalar = 0x12,

    /// Array of Scalar field elements
    ScalarArray = 0x13,

    /// A Merkle path
    MerklePath = 0x20,

    /// Unsigned 32-bit integer
    Uint32 = 0x30,

    /// Unsigned 64-bit integer
    Uint64 = 0x31,

    /// Intermediate type, should never appear in the result
    Dummy = 0xff,
}

impl Type {
    pub fn from_repr(b: u8) -> Self {
        match b {
            0x00 => Self::EcPoint,
            0x01 => Self::EcFixedPoint,
            0x02 => Self::EcFixedPointShort,
            0x03 => Self::EcFixedPointBase,
            0x10 => Self::Base,
            0x11 => Self::BaseArray,
            0x12 => Self::Scalar,
            0x13 => Self::ScalarArray,
            0x20 => Self::MerklePath,
            0x30 => Self::Uint32,
            0x31 => Self::Uint64,
            _ => unimplemented!(),
        }
    }
}
