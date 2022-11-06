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

use darkfi_serial::{serialize, SerialDecodable, SerialEncodable};
use pasta_curves::{group::ff::PrimeField, pallas};

#[derive(Copy, Clone, Debug, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct ContractId(pallas::Base);

impl ContractId {
    pub fn new(contract_id: pallas::Base) -> Self {
        Self(contract_id)
    }

    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    pub fn from_bytes(x: [u8; 32]) -> Self {
        // FIXME: Handle Option
        Self(pallas::Base::from_repr(x).unwrap())
    }

    /// `blake3(self || tree_name)` is used in datbases to have a
    /// fixed-size name for a contract's state db.
    pub fn hash_state_id(&self, tree_name: &str) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&serialize(self));
        hasher.update(&tree_name.as_bytes());
        let id = hasher.finalize();
        *id.as_bytes()
    }
}

impl std::fmt::Display for ContractId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        // base58 encoding
        let contractid: String = bs58::encode(self.0.to_repr()).into_string();
        write!(f, "{}", contractid)
    }
}
