/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use num_bigint::BigUint;

use super::{PoseidonFp, SparseMerkleTree, StorageAdapter, SMT_FP_DEPTH};
use crate::{
    crypto::pasta_prelude::*,
    error::ContractResult,
    msg,
    pasta::pallas,
    wasm::db::{db_del, db_get, db_set, DbHandle},
};

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

    fn put(&mut self, key: BigUint, value: pallas::Base) -> ContractResult {
        db_set(self.db, &key.to_bytes_le(), &value.to_repr())
    }

    fn get(&self, key: &BigUint) -> Option<pallas::Base> {
        let Ok(value) = db_get(self.db, &key.to_bytes_le()) else {
            msg!("[WasmDbStorage] get() for DB failed");
            return None
        };

        let value = value?;

        let mut repr = [0; 32];
        repr.copy_from_slice(&value);

        pallas::Base::from_repr(repr).into()
    }

    fn del(&mut self, key: &BigUint) -> ContractResult {
        db_del(self.db, &key.to_bytes_le())
    }
}
