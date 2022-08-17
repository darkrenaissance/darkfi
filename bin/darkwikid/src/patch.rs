use std::{cmp::Ordering, io};

use serde::{Deserialize, Serialize};

use darkfi::util::serial::{Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt};

use crate::str_to_chars;

#[derive(PartialEq, Eq, Serialize, Deserialize, Clone, Debug)]
pub enum OpMethod {
    Delete(u64),
    Insert(String),
    Retain(u64),
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Clone, Debug)]
pub struct OpMethods(pub Vec<OpMethod>);

#[derive(PartialEq, Eq, SerialEncodable, SerialDecodable, Serialize, Deserialize, Clone, Debug)]
pub struct Patch {
    author: String,
    id: String,
    base: String,
    ops: OpMethods,
}

impl std::string::ToString for Patch {
    fn to_string(&self) -> String {
        let mut st = vec![];
        let mut index: usize = 0;

        st.extend(str_to_chars(&self.base));
        for op in self.ops.0.iter() {
            match op {
                OpMethod::Retain(n) => {
                    index += *n as usize;
                }
                OpMethod::Delete(n) => {
                    if (index + (*n as usize)) > st.len() {
                        if index < st.len() {
                            st.drain(index..st.len());
                        }
                    } else {
                        st.drain(index..(index + *n as usize));
                    }
                }
                OpMethod::Insert(insert) => {
                    let chars = str_to_chars(insert);
                    for c in chars {
                        if index > st.len() {
                            st.push(c);
                        } else {
                            st.insert(index, c);
                            index += 1;
                        }
                    }
                }
            }
        }

        st.join("")
    }
}

impl Patch {
    pub fn new(id: &str, author: &str) -> Self {
        Self {
            id: id.to_string(),
            ops: OpMethods(vec![]),
            base: String::new(),
            author: author.to_string(),
        }
    }

    pub fn add_op(&mut self, method: &OpMethod) {
        match method {
            OpMethod::Delete(n) => {
                if *n == 0 {
                    return
                }

                if let Some(OpMethod::Delete(i)) = self.ops.0.last_mut() {
                    *i += n;
                } else {
                    self.ops.0.push(method.to_owned());
                }
            }
            OpMethod::Insert(insert) => {
                if insert.is_empty() {
                    return
                }

                if let Some(OpMethod::Insert(s)) = self.ops.0.last_mut() {
                    *s += insert;
                } else {
                    self.ops.0.push(OpMethod::Insert(insert.to_owned()));
                }
            }
            OpMethod::Retain(n) => {
                if *n == 0 {
                    return
                }

                if let Some(OpMethod::Retain(i)) = self.ops.0.last_mut() {
                    *i += n;
                } else {
                    self.ops.0.push(method.to_owned());
                }
            }
        }
    }

    fn insert(&mut self, st: &str) {
        self.add_op(&OpMethod::Insert(st.into()));
    }

    fn retain(&mut self, n: u64) {
        self.add_op(&OpMethod::Retain(n));
    }

    fn delete(&mut self, n: u64) {
        self.add_op(&OpMethod::Delete(n));
    }

    pub fn set_base(&mut self, base: &str) {
        self.base = base.to_owned();
    }

    pub fn set_ops(&mut self, ops: OpMethods) {
        self.ops = ops;
    }

    pub fn extend_ops(&mut self, ops: OpMethods) {
        self.ops.0.extend(ops.0);
    }

    pub fn base_empty(&self) -> bool {
        self.base.is_empty()
    }

    pub fn id(&self) -> String {
        self.id.clone()
    }

    pub fn ops(&self) -> OpMethods {
        self.ops.clone()
    }

    //
    // these two functions are imported from this library
    // https://github.com/spebern/operational-transform-rs
    // with some major modification
    //
    // TODO need more work to get better performance with iterators
    pub fn transform(&self, other: &Self) -> Self {
        let mut new_patch = Self::new(&self.id, &self.author);
        new_patch.set_base(&self.base);

        let mut ops1 = self.ops.0.iter().cloned();
        let mut ops2 = other.ops.0.iter().cloned();

        let mut op1 = ops1.next();
        let mut op2 = ops2.next();
        loop {
            if op2.is_none() {
                break
            }

            if op1.is_none() {
                new_patch.add_op(op2.as_ref().unwrap());
                op2 = ops2.next();
                continue
            }

            match (op1.as_ref().unwrap(), op2.as_ref().unwrap()) {
                (OpMethod::Insert(s), _) => {
                    new_patch.retain(str_to_chars(s).len() as _);
                    op1 = ops1.next();
                }
                (_, OpMethod::Insert(s)) => {
                    new_patch.insert(s);
                    op2 = ops2.next();
                }
                (OpMethod::Retain(i), OpMethod::Retain(j)) => match i.cmp(j) {
                    Ordering::Less => {
                        new_patch.retain(*i);
                        op2 = Some(OpMethod::Retain(j - *i));
                        op1 = ops1.next();
                    }
                    Ordering::Greater => {
                        new_patch.retain(*j);
                        op1 = Some(OpMethod::Retain(i - j));
                        op2 = ops2.next();
                    }
                    Ordering::Equal => {
                        new_patch.retain(*i);
                        op1 = ops1.next();
                        op2 = ops2.next();
                    }
                },
                (OpMethod::Delete(i), OpMethod::Delete(j)) => match i.cmp(j) {
                    Ordering::Less => {
                        op2 = Some(OpMethod::Delete(j - *i));
                        op1 = ops1.next();
                    }
                    Ordering::Greater => {
                        op1 = Some(OpMethod::Delete(i - j));
                        op2 = ops2.next();
                    }
                    Ordering::Equal => {
                        op1 = ops1.next();
                        op2 = ops2.next();
                    }
                },
                (OpMethod::Delete(i), OpMethod::Retain(j)) => match i.cmp(j) {
                    Ordering::Less => {
                        op2 = Some(OpMethod::Retain(j - *i));
                        op1 = ops1.next();
                    }
                    Ordering::Greater => {
                        op1 = Some(OpMethod::Delete(i - j));
                        op2 = ops2.next();
                    }
                    Ordering::Equal => {
                        op1 = ops1.next();
                        op2 = ops2.next();
                    }
                },
                (OpMethod::Retain(i), OpMethod::Delete(j)) => match i.cmp(j) {
                    Ordering::Less => {
                        new_patch.delete(*i);
                        op2 = Some(OpMethod::Delete(j - i));
                        op1 = ops1.next();
                    }
                    Ordering::Greater => {
                        new_patch.delete(*j);
                        op1 = Some(OpMethod::Retain(i - j));
                        op2 = ops2.next();
                    }
                    Ordering::Equal => {
                        new_patch.delete(*i);
                        op1 = ops1.next();
                        op2 = ops2.next();
                    }
                },
            }
        }

        new_patch
    }

    // TODO need more work to get better performance with iterators
    pub fn merge(&mut self, other: &Self) -> Self {
        let ops1 = self.ops.0.clone();
        let mut ops1 = ops1.iter().cloned();
        let mut ops2 = other.ops.0.iter().cloned();

        let mut new_patch = Self::new(&self.id, &self.author);
        new_patch.set_base(&self.base);

        let mut op1 = ops1.next();
        let mut op2 = ops2.next();

        loop {
            if op2.is_none() {
                break
            }

            if op1.is_none() {
                new_patch.add_op(op2.as_ref().unwrap());
                op2 = ops2.next();
                continue
            }

            match (op1.as_ref().unwrap(), op2.as_ref().unwrap()) {
                (OpMethod::Delete(i), _) => {
                    new_patch.delete(*i);
                    op1 = ops1.next();
                }
                (_, OpMethod::Insert(s)) => {
                    new_patch.insert(s);
                    op2 = ops2.next();
                }
                (OpMethod::Retain(i), OpMethod::Retain(j)) => match i.cmp(j) {
                    Ordering::Less => {
                        new_patch.retain(*i);
                        op2 = Some(OpMethod::Retain(*j - i));
                        op1 = ops1.next();
                    }
                    Ordering::Greater => {
                        new_patch.retain(*j);
                        op1 = Some(OpMethod::Retain(i - *j));
                        op2 = ops2.next();
                    }
                    Ordering::Equal => {
                        new_patch.retain(*i);
                        op1 = ops1.next();
                        op2 = ops2.next();
                    }
                },
                (OpMethod::Insert(s), OpMethod::Delete(j)) => {
                    let chars = str_to_chars(s);
                    let chars_len = chars.len() as u64;
                    match chars_len.cmp(j) {
                        Ordering::Less => {
                            op1 = ops1.next();
                            op2 = Some(OpMethod::Delete(j - chars_len));
                        }
                        Ordering::Greater => {
                            let st = chars.into_iter().skip(*j as usize).collect();
                            op1 = Some(OpMethod::Insert(st));
                            op2 = ops2.next();
                        }
                        Ordering::Equal => {
                            op1 = ops1.next();
                            op2 = ops2.next();
                        }
                    }
                }
                (OpMethod::Insert(s), OpMethod::Retain(j)) => {
                    let chars = str_to_chars(s);
                    let chars_len = chars.len() as u64;
                    match chars_len.cmp(j) {
                        Ordering::Less => {
                            new_patch.insert(s);
                            op1 = ops1.next();
                            op2 = Some(OpMethod::Retain(*j - chars_len));
                        }
                        Ordering::Greater => {
                            let st = chars.into_iter().take(*j as usize).collect::<String>();
                            new_patch.insert(&st);
                            op1 = Some(OpMethod::Insert(st));
                            op2 = ops2.next();
                        }
                        Ordering::Equal => {
                            new_patch.insert(s);
                            op1 = ops1.next();
                            op2 = ops2.next();
                        }
                    }
                }
                (OpMethod::Retain(i), OpMethod::Delete(j)) => match i.cmp(j) {
                    Ordering::Less => {
                        new_patch.delete(*i);
                        op2 = Some(OpMethod::Delete(*j - *i));
                        op1 = ops1.next();
                    }
                    Ordering::Greater => {
                        new_patch.delete(*j);
                        op1 = Some(OpMethod::Retain(*i - *j));
                        op2 = ops2.next();
                    }
                    Ordering::Equal => {
                        new_patch.delete(*j);
                        op1 = ops1.next();
                        op2 = ops2.next();
                    }
                },
            };
        }

        new_patch
    }
}

impl Decodable for OpMethod {
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
            _ => Err(darkfi::Error::ParseFailed("Parse OpMethod failed")),
        }
    }
}

impl Encodable for OpMethod {
    fn encode<S: io::Write>(&self, mut s: S) -> darkfi::Result<usize> {
        let len: usize = match self {
            Self::Delete(i) => (0_u8).encode(&mut s)? + i.encode(&mut s)?,
            Self::Insert(t) => (1_u8).encode(&mut s)? + t.encode(&mut s)?,
            Self::Retain(i) => (2_u8).encode(&mut s)? + i.encode(&mut s)?,
        };
        Ok(len)
    }
}

impl Encodable for OpMethods {
    fn encode<S: io::Write>(&self, mut s: S) -> darkfi::Result<usize> {
        let mut len = 0;
        len += VarInt(self.0.len() as u64).encode(&mut s)?;
        for c in self.0.iter() {
            len += c.encode(&mut s)?;
        }
        Ok(len)
    }
}

impl Decodable for OpMethods {
    fn decode<D: io::Read>(mut d: D) -> darkfi::Result<Self> {
        let len = VarInt::decode(&mut d)?.0;
        let mut ret = Vec::with_capacity(len as usize);
        for _ in 0..len {
            ret.push(Decodable::decode(&mut d)?);
        }
        Ok(Self(ret))
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
    fn test_to_string() {
        let mut patch = Patch::new(&gen_id(30), "");
        patch.set_base("text example\n hello");
        patch.retain(14);
        patch.delete(5);
        patch.insert("hey");

        assert_eq!(patch.to_string(), "text example\n hey");
    }

    #[test]
    fn test_merge() {
        let mut patch_init = Patch::new(&gen_id(30), "");
        let base = "text example\n hello";
        patch_init.set_base(base);

        let mut patch1 = patch_init.clone();
        patch1.retain(14);
        patch1.delete(5);
        patch1.insert("hey");

        let mut patch2 = patch_init.clone();
        patch2.retain(14);
        patch2.delete(5);
        patch2.insert("test");

        patch1.merge(&patch2);

        let patch3 = patch1.merge(&patch2);

        assert_eq!(patch3.to_string(), "text example\n test");

        let mut patch1 = patch_init.clone();
        patch1.retain(5);
        patch1.delete(7);
        patch1.insert("ex");
        patch1.retain(7);

        let mut patch2 = patch_init.clone();
        patch2.delete(4);
        patch2.insert("new");
        patch2.retain(13);

        let patch3 = patch1.merge(&patch2);

        assert_eq!(patch3.to_string(), "new ex\n hello");
    }

    #[test]
    fn test_transform() {
        let mut patch_init = Patch::new(&gen_id(30), "");
        let base = "text example\n hello";
        patch_init.set_base(base);

        let mut patch1 = patch_init.clone();
        patch1.retain(14);
        patch1.delete(5);
        patch1.insert("hey");

        let mut patch2 = patch_init.clone();
        patch2.retain(14);
        patch2.delete(5);
        patch2.insert("test");

        let patch3 = patch1.transform(&patch2);
        let patch4 = patch1.merge(&patch3);

        assert_eq!(patch4.to_string(), "text example\n heytest");

        let mut patch1 = patch_init.clone();
        patch1.retain(5);
        patch1.delete(7);
        patch1.insert("ex");
        patch1.retain(7);

        let mut patch2 = patch_init.clone();
        patch2.delete(4);
        patch2.insert("new");
        patch2.retain(13);

        let patch3 = patch1.transform(&patch2);
        let patch4 = patch1.merge(&patch3);

        assert_eq!(patch4.to_string(), "new ex\n hello");
    }

    #[test]
    fn test_serialize() {
        // serialize & deserialize OpMethod
        let op_method = OpMethod::Delete(3);

        let op_method_ser = serialize(&op_method);
        let op_method_deser = deserialize(&op_method_ser).unwrap();

        assert_eq!(op_method, op_method_deser);

        // serialize & deserialize Patch
        let mut patch = Patch::new(&gen_id(30), "");
        patch.insert("hello");
        patch.delete(2);

        let patch_ser = serialize(&patch);
        let patch_deser = deserialize(&patch_ser).unwrap();

        assert_eq!(patch, patch_deser);
    }
}
