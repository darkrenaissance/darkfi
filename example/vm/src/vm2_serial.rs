use std::io;

use crate::error::{Error, Result};
use crate::impl_vec;
use crate::serial::{Decodable, Encodable, ReadExt, VarInt};
use crate::vm2::{ZkBinary, ZkContract, ZkFunctionCall, ZkType};

impl_vec!((String, ZkType));
impl_vec!(ZkFunctionCall);
impl_vec!((String, ZkContract));

impl Encodable for ZkType {
    fn encode<S: io::Write>(&self, _s: S) -> Result<usize> {
        unimplemented!();
        //Ok(0)
    }
}

impl Decodable for ZkType {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let op_type = ReadExt::read_u8(&mut d)?;
        match op_type {
            0 => Ok(Self::Base),
            1 => Ok(Self::Scalar),
            2 => Ok(Self::EcPoint),
            3 => Ok(Self::EcFixedPoint),
            _i => Err(Error::BadOperationType),
        }
    }
}

impl Encodable for ZkFunctionCall {
    fn encode<S: io::Write>(&self, _s: S) -> Result<usize> {
        unimplemented!();
        //Ok(0)
    }
}

impl Decodable for ZkFunctionCall {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let func_id = ReadExt::read_u8(&mut d)?;
        match func_id {
            0 => Ok(Self::PoseidonHash(
                ReadExt::read_u32(&mut d)? as usize,
                ReadExt::read_u32(&mut d)? as usize,
            )),
            1 => Ok(Self::Add(
                ReadExt::read_u32(&mut d)? as usize,
                ReadExt::read_u32(&mut d)? as usize,
            )),
            2 => Ok(Self::ConstrainInstance(ReadExt::read_u32(&mut d)? as usize)),
            3 => Ok(Self::EcMulShort(
                ReadExt::read_u32(&mut d)? as usize,
                ReadExt::read_u32(&mut d)? as usize,
            )),
            4 => Ok(Self::EcMul(
                ReadExt::read_u32(&mut d)? as usize,
                ReadExt::read_u32(&mut d)? as usize,
            )),
            5 => Ok(Self::EcAdd(
                ReadExt::read_u32(&mut d)? as usize,
                ReadExt::read_u32(&mut d)? as usize,
            )),
            6 => Ok(Self::EcGetX(ReadExt::read_u32(&mut d)? as usize)),
            7 => Ok(Self::EcGetY(ReadExt::read_u32(&mut d)? as usize)),
            _i => Err(Error::BadOperationType),
        }
    }
}

impl Encodable for ZkBinary {
    fn encode<S: io::Write>(&self, _s: S) -> Result<usize> {
        unimplemented!();
        //Ok(0)
    }
}

impl Decodable for ZkBinary {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            constants: Decodable::decode(&mut d)?,
            contracts: Vec::<(String, ZkContract)>::decode(&mut d)?
                .into_iter()
                .collect(),
        })
    }
}

impl Encodable for ZkContract {
    fn encode<S: io::Write>(&self, _s: S) -> Result<usize> {
        unimplemented!();
        //Ok(0)
    }
}

impl Decodable for ZkContract {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            witness: Decodable::decode(&mut d)?,
            code: Decodable::decode(&mut d)?,
        })
    }
}
