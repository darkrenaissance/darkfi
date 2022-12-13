/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{cmp::Ordering, io};

use darkfi_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt};
use dryoc::constants::CRYPTO_SECRETBOX_NONCEBYTES;
use serde::{Deserialize, Serialize};

use darkfi::util::{
    cli::{fg_green, fg_red},
    time::Timestamp,
};

use crate::util::str_to_chars;

#[derive(PartialEq, Eq, Serialize, Deserialize, Clone, Debug)]
pub enum OpMethod {
    Delete(u64),
    Insert(String),
    Retain(u64),
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Clone, Debug)]
pub struct OpMethods(pub Vec<OpMethod>);

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct EncryptedPatch {
    pub nonce: [u8; CRYPTO_SECRETBOX_NONCEBYTES],
    pub ciphertext: Vec<u8>,
}

#[derive(PartialEq, Eq, SerialEncodable, SerialDecodable, Serialize, Deserialize, Clone, Debug)]
pub struct Patch {
    pub path: String,
    pub author: String,
    pub id: String,
    pub base: String,
    pub timestamp: Timestamp,
    pub workspace: String,
    ops: OpMethods,
}

impl std::string::ToString for Patch {
    fn to_string(&self) -> String {
        if self.ops.0.is_empty() {
            return self.base.clone()
        }

        let mut st = vec![];
        st.extend(str_to_chars(&self.base));
        let st = &mut st.iter();

        let mut new_st: Vec<&str> = vec![];

        for op in self.ops.0.iter() {
            match op {
                OpMethod::Retain(n) => {
                    for c in st.take(*n as usize) {
                        new_st.push(c);
                    }
                }
                OpMethod::Delete(n) => {
                    for _ in 0..*n {
                        st.next();
                    }
                }
                OpMethod::Insert(insert) => {
                    let chars = str_to_chars(insert);
                    new_st.extend(chars);
                }
            }
        }

        new_st.join("")
    }
}

impl Patch {
    pub fn new(path: &str, id: &str, author: &str, workspace: &str) -> Self {
        Self {
            path: path.to_string(),
            id: id.to_string(),
            ops: OpMethods(vec![]),
            base: String::new(),
            workspace: workspace.to_string(),
            author: author.to_string(),
            timestamp: Timestamp::current_time(),
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

    pub fn set_ops(&mut self, ops: OpMethods) {
        self.ops = ops;
    }

    pub fn extend_ops(&mut self, ops: OpMethods) {
        self.ops.0.extend(ops.0);
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
        let mut new_patch = Self::new(&self.path, &self.id, &self.author, "");
        new_patch.base = self.base.clone();

        let mut ops1 = self.ops.0.iter().cloned();
        let mut ops2 = other.ops.0.iter().cloned();

        let mut op1 = ops1.next();
        let mut op2 = ops2.next();
        loop {
            match (&op1, &op2) {
                (None, None) => break,
                (None, Some(op)) => {
                    new_patch.add_op(op);
                    op2 = ops2.next();
                    continue
                }
                (Some(op), None) => {
                    new_patch.add_op(op);
                    op1 = ops1.next();
                    continue
                }
                _ => {}
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

        let mut new_patch = Self::new(&self.path, &self.id, &self.author, "");
        new_patch.base = self.base.clone();

        let mut op1 = ops1.next();
        let mut op2 = ops2.next();

        loop {
            match (&op1, &op2) {
                (None, None) => break,
                (None, Some(op)) => {
                    new_patch.add_op(op);
                    op2 = ops2.next();
                    continue
                }
                (Some(op), None) => {
                    new_patch.add_op(op);
                    op1 = ops1.next();
                    continue
                }
                _ => {}
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

    pub fn colorize(&self) -> String {
        if self.ops.0.is_empty() {
            return fg_green(&self.base)
        }

        let mut st = vec![];
        st.extend(str_to_chars(&self.base));
        let st = &mut st.iter();

        let mut colorized_str: Vec<String> = vec![];

        for op in self.ops.0.iter() {
            match op {
                OpMethod::Retain(n) => {
                    for c in st.take(*n as usize) {
                        colorized_str.push(c.to_string());
                    }
                }
                OpMethod::Delete(n) => {
                    let mut deleted_part = vec![];
                    for _ in 0..*n {
                        let s = st.next();
                        if let Some(s) = s {
                            deleted_part.push(s.to_string());
                        }
                    }
                    colorized_str.push(fg_red(&deleted_part.join("")));
                }
                OpMethod::Insert(insert) => {
                    let chars = str_to_chars(insert);
                    colorized_str.push(fg_green(&chars.join("")))
                }
            }
        }

        colorized_str.join("")
    }
}

impl Decodable for OpMethod {
    fn decode<D: io::Read>(mut d: D) -> core::result::Result<Self, io::Error> {
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
            _ => Err(io::Error::new(io::ErrorKind::Other, "Parse OpMethod failed")),
        }
    }
}

impl Encodable for OpMethod {
    fn encode<S: io::Write>(&self, mut s: S) -> core::result::Result<usize, io::Error> {
        let len: usize = match self {
            Self::Delete(i) => (0_u8).encode(&mut s)? + i.encode(&mut s)?,
            Self::Insert(t) => (1_u8).encode(&mut s)? + t.encode(&mut s)?,
            Self::Retain(i) => (2_u8).encode(&mut s)? + i.encode(&mut s)?,
        };
        Ok(len)
    }
}

impl Encodable for OpMethods {
    fn encode<S: io::Write>(&self, mut s: S) -> core::result::Result<usize, io::Error> {
        let mut len = 0;
        len += VarInt(self.0.len() as u64).encode(&mut s)?;
        for c in self.0.iter() {
            len += c.encode(&mut s)?;
        }
        Ok(len)
    }
}

impl Decodable for OpMethods {
    fn decode<D: io::Read>(mut d: D) -> core::result::Result<Self, io::Error> {
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
    use darkfi::raft::gen_id;
    use darkfi_serial::{deserialize, serialize};

    #[test]
    fn test_to_string() {
        let mut patch = Patch::new("", &gen_id(30), "", "");
        patch.base = "text example\n hello".to_string();
        patch.retain(14);
        patch.delete(5);
        patch.insert("hey");

        assert_eq!(patch.to_string(), "text example\n hey");
    }

    #[test]
    fn test_merge() {
        let mut patch_init = Patch::new("", &gen_id(30), "", "");
        let base = "text example\n hello";
        patch_init.base = base.to_string();

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

        let mut patch2 = patch_init;
        patch2.delete(4);
        patch2.insert("new");
        patch2.retain(13);

        let patch3 = patch1.merge(&patch2);

        assert_eq!(patch3.to_string(), "new ex\n hello");
    }

    #[test]
    fn test_transform() {
        let mut patch_init = Patch::new("", &gen_id(30), "", "");
        let base = "text example\n hello";
        patch_init.base = base.to_string();

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

        let mut patch2 = patch_init;
        patch2.delete(4);
        patch2.insert("new");
        patch2.retain(13);

        let patch3 = patch1.transform(&patch2);
        let patch4 = patch1.merge(&patch3);

        assert_eq!(patch4.to_string(), "new ex\n hello");
    }

    #[test]
    fn test_transform2() {
        let mut patch_init = Patch::new("", &gen_id(30), "", "");
        let base = "#hello\n hello";
        patch_init.base = base.to_string();

        let mut patch1 = patch_init.clone();
        patch1.retain(13);
        patch1.insert(" world");

        let mut patch2 = patch_init;
        patch2.retain(1);
        patch2.delete(5);
        patch2.insert("this is the title");
        patch2.retain(7);
        patch2.insert("\n this is the content");

        let patch3 = patch1.transform(&patch2);
        let patch4 = patch1.merge(&patch3);

        assert_eq!(patch4.to_string(), "#this is the title\n hello world\n this is the content");
    }

    #[test]
    fn test_serialize() {
        // serialize & deserialize OpMethod
        let op_method = OpMethod::Delete(3);

        let op_method_ser = serialize(&op_method);
        let op_method_deser = deserialize(&op_method_ser).unwrap();

        assert_eq!(op_method, op_method_deser);

        // serialize & deserialize Patch
        let mut patch = Patch::new("", &gen_id(30), "", "");
        patch.insert("hello");
        patch.delete(2);

        let patch_ser = serialize(&patch);
        let patch_deser = deserialize(&patch_ser).unwrap();

        assert_eq!(patch, patch_deser);
    }
}
