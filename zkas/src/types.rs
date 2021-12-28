/// Types supported by the VM
#[derive(Clone, Debug)]
pub enum Type {
    EcFixedPoint = 0x00,
    Base = 0x01,
    Scalar = 0x02,
    MerklePath = 0x03,
}

#[derive(Clone, Debug)]
pub struct Constant {
    pub name: String,
    pub typ: Type,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug)]
pub struct Witness {
    pub name: String,
    pub typ: Type,
    pub line: usize,
    pub column: usize,
}
