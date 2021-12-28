// Opcodes supported by the VM
pub enum OpCode {
    EcAdd = 0x00,
    EcMul = 0x01,
    EcMulShort = 0x02,
    EcGetX = 0x03,
    EcGetY = 0x04,

    PoseidonHash = 0x10,

    CalculateMerkleRoot = 0x20,

    ConstrainInstance = 0xf0,
}
