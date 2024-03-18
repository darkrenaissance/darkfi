use super::{PoseidonFp, SparseMerkleTree, StorageAdapter, SMT_FP_DEPTH};
use crate::{
    crypto::pasta_prelude::*,
    db::{db_get, db_set, DbHandle},
    msg,
    pasta::pallas,
};
use num_bigint::BigUint;

pub type SmtWasmFp = SparseMerkleTree<
    'static,
    SMT_FP_DEPTH,
    { SMT_FP_DEPTH + 1 },
    pallas::Base,
    PoseidonFp,
    SmtWasmDbStorage,
>;

pub struct SmtWasmDbStorage {
    db: DbHandle,
}

impl SmtWasmDbStorage {
    pub fn new(db: DbHandle) -> Self {
        Self { db }
    }
}

impl StorageAdapter for SmtWasmDbStorage {
    type Value = pallas::Base;

    fn put(&mut self, key: BigUint, value: pallas::Base) -> bool {
        db_set(self.db, &key.to_bytes_le(), &value.to_repr()).is_ok()
    }
    fn get(&self, key: &BigUint) -> Option<pallas::Base> {
        let Ok(value) = db_get(self.db, &key.to_bytes_le()) else {
            msg!("[WasmDbStorage] get() for DB failed");
            return None
        };
        let Some(value) = value else { return None };

        let mut repr = [0; 32];
        repr.copy_from_slice(&value);
        let value = pallas::Base::from_repr(repr);
        if value.is_none().into() {
            None
        } else {
            Some(value.unwrap())
        }
    }
}
