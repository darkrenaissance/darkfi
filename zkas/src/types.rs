/// Types supported by the VM
#[derive(Copy, Clone, PartialEq, Debug)]
#[repr(u8)]
pub enum Type {
    /// Elliptic curve point
    EcPoint = 0x00,

    /// Elliptic curve fixed point (a constant)
    EcFixedPoint = 0x01,

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

    /// Intermediate type, should never appear in the result
    Dummy = 0xff,
}

impl Type {
    pub fn from_repr(b: u8) -> Self {
        match b {
            0 => Self::EcPoint,
            1 => Self::EcFixedPoint,
            16 => Self::Base,
            17 => Self::BaseArray,
            18 => Self::Scalar,
            19 => Self::ScalarArray,
            32 => Self::MerklePath,
            48 => Self::Uint32,
            _ => unimplemented!(),
        }
    }
}
