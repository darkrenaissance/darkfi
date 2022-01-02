/// Types supported by the VM
#[derive(Clone, Debug)]
pub enum Type {
    EcPoint = 0x00,
    EcFixedPoint = 0x01,
    Base = 0x10,
    Scalar = 0x11,
    MerklePath = 0x20,
}
