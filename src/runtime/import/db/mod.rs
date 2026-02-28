/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use darkfi_sdk::crypto::contract_id::ContractId;

/// Internal wasm runtime API for sled trees or tx-local dbs
#[derive(PartialEq)]
pub struct DbHandle {
    pub contract_id: ContractId,
    pub tree: [u8; 32],
}

impl DbHandle {
    pub fn new(contract_id: ContractId, tree: [u8; 32]) -> Self {
        Self { contract_id, tree }
    }
}

pub(crate) mod db_init;
pub(crate) use db_init::db_init;

pub(crate) mod db_lookup;
pub(crate) use db_lookup::db_lookup;

pub(crate) mod db_set;
pub(crate) use db_set::db_set;

pub(crate) mod db_del;
pub(crate) use db_del::db_del;

pub(crate) mod db_get;
pub(crate) use db_get::db_get;

pub(crate) mod db_contains_key;
pub(crate) use db_contains_key::db_contains_key;

pub(crate) mod zkas_db_set;
pub(crate) use zkas_db_set::zkas_db_set;

mod util;
