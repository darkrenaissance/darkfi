use std::{io, result::Result};

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use darkfi::util::serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};

use crate::error::DarkWikiError;

#[derive(PartialEq, Serialize, Deserialize, Clone, Debug)]
pub enum OperationMethod {
    Delete(u64),
    Insert(String),
    Retain(u64),
}

#[derive(PartialEq, Serialize, Deserialize, SerialEncodable, SerialDecodable, Clone, Debug)]
pub struct Operation {
    id: String,
    method: OperationMethod,
}

impl Operation {
    pub fn id(&self) -> String {
        self.id.clone()
    }
}

#[derive(PartialEq, Serialize, Deserialize, Clone, Debug)]
pub struct Sequence {
    id: String,
    operations: Vec<OperationMethod>,
    len: u64,
}

impl Sequence {
    pub fn new(id: &str) -> Self {
        Self { id: id.to_string(), operations: vec![], len: 0 }
    }
    ///
    /// Apply all operations to the provided &str
    /// Return the final String
    ///
    pub fn apply(&self, s: &str) -> String {
        let mut st = vec![];
        let mut chars = s.graphemes(true).collect::<Vec<&str>>();
        for op in &self.operations {
            match op {
                OperationMethod::Retain(n) => {
                    st.extend(chars[..(*n as usize)].to_vec());
                }
                OperationMethod::Delete(n) => {
                    chars.drain(0..(*n as usize));
                }
                OperationMethod::Insert(insert) => {
                    st.extend(insert.graphemes(true).collect::<Vec<&str>>());
                }
            }
        }

        st.join("")
    }

    ///
    /// Add new operation
    /// Return AddOperationFailed error if failed
    ///
    pub fn add_op(&mut self, op: &Operation) -> Result<(), DarkWikiError> {
        match &op.method {
            OperationMethod::Delete(n) => {
                if *n == 0 {
                    return Ok(())
                }
            }
            OperationMethod::Insert(insert) => {
                if insert.is_empty() {
                    return Ok(())
                }
            }
            OperationMethod::Retain(n) => {
                if *n == 0 {
                    return Ok(())
                }
            }
        }

        self.operations.push(op.method.clone());
        Ok(())
    }

    ///
    /// Insert string at `n` position with Insert Operation
    /// Return AddOperationFailed if failed
    ///
    pub fn insert(&mut self, st: &str) -> Result<Operation, DarkWikiError> {
        let method = OperationMethod::Insert(st.into());
        let op = Operation { id: self.id.clone(), method };
        self.add_op(&op)?;
        Ok(op)
    }

    ///
    /// Move the position of cursor
    /// Return AddOperationFailed if failed
    ///
    pub fn retain(&mut self, n: u64) -> Result<Operation, DarkWikiError> {
        let method = OperationMethod::Retain(n);
        let op = Operation { id: self.id.clone(), method };
        self.add_op(&op)?;
        Ok(op)
    }

    ///
    /// Delete string at `n` position with Delete Operation
    /// Return AddOperationFailed if failed
    ///
    pub fn delete(&mut self, n: u64) -> Result<Operation, DarkWikiError> {
        let method = OperationMethod::Delete(n);
        let op = Operation { id: self.id.clone(), method };
        self.add_op(&op)?;
        Ok(op)
    }
}

impl Encodable for OperationMethod {
    fn encode<S: io::Write>(&self, mut s: S) -> darkfi::Result<usize> {
        let len: usize = match self {
            Self::Delete(i) => (0 as u8).encode(&mut s)? + i.encode(&mut s)?,
            Self::Insert(t) => (1 as u8).encode(&mut s)? + t.encode(&mut s)?,
            Self::Retain(i) => (2 as u8).encode(&mut s)? + i.encode(&mut s)?,
        };
        Ok(len)
    }
}

impl Decodable for OperationMethod {
    fn decode<D: io::Read>(mut d: D) -> darkfi::Result<Self> {
        let com: u8 = Decodable::decode(&mut d)?;
        match com {
            0 => {
                let i: u64 = Decodable::decode(&mut d)?;
                Ok(Self::Delete(i))
            }
            1 => {
                let t: String = Decodable::decode(d)?;
                Ok(Self::Insert(t))
            }
            2 => {
                let i: u64 = Decodable::decode(&mut d)?;
                Ok(Self::Retain(i))
            }
            _ => Err(darkfi::Error::ParseFailed("Parse OperationMethod failed")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkfi::util::{
        gen_id,
        serial::{deserialize, serialize},
    };

    #[test]
    fn test_seq() {
        //
        // English
        //
        let _t = "this is the first paragraph";
        let mut seq = Sequence::new(&gen_id(30));

        //
        // Korean
        //
        let _t = "안녕하십니까";
        let mut seq = Sequence::new(&gen_id(30));

        //
        // Arabic
        //
        let _t = "عربي";
        let mut seq = Sequence::new(&gen_id(30));
    }

    #[test]
    fn test_serialize() {
        let op_method = OperationMethod::Delete(3);

        let op_method_ser = serialize(&op_method);
        let op_method_deser = deserialize(&op_method_ser).unwrap();

        assert_eq!(op_method, op_method_deser);
    }
}
