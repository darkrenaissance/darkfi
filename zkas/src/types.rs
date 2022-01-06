/// Types supported by the VM
#[derive(Copy, Clone, PartialEq, Debug)]
#[repr(u8)]
pub enum Type {
    EcPoint = 0x00,

    EcFixedPoint = 0x01,

    Base = 0x10,
    BaseArray = 0x11,

    Scalar = 0x12,
    ScalarArray = 0x13,

    MerklePath = 0x20,

    Uint32 = 0x30,

    Dummy = 0xff,
}
