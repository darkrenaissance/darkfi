use std::collections::{hash_map::Keys, HashMap};

use crate::types::TypeId;

#[derive(Debug, Clone)]
pub struct Line {
    pub tokens: Vec<String>,
    pub orig: String,
    pub number: u32,
}

impl Line {
    pub fn new(tokens: Vec<String>, orig: String, number: u32) -> Self {
        Line {
            tokens,
            orig,
            number,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Constants {
    pub table: Vec<TypeId>,
    pub map: HashMap<String, usize>,
}

impl Constants {
    pub fn new() -> Self {
        Constants {
            table: vec![],
            map: HashMap::new(),
        }
    }

    pub fn add(&mut self, variable: String, type_id: TypeId) {
        let idx = self.table.len();
        self.table.push(type_id);
        self.map.insert(variable, idx);
    }

    pub fn lookup(&self, variable: String) -> TypeId {
        if let Some(idx) = self.map.get(variable.as_str()) {
            return self.table[*idx];
        }

        panic!();
    }

    pub fn variables(&self) -> Keys<'_, String, usize> {
        self.map.keys()
    }
}
