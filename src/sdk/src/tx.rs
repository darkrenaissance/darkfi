/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use std::fmt::{self, Debug};

#[cfg(feature = "async")]
use darkfi_serial::async_trait;
use darkfi_serial::{SerialDecodable, SerialEncodable};

use super::crypto::ContractId;

#[derive(Clone, Debug, PartialEq)]
// We have to introduce a type rather than using an alias so we can implement Display
pub struct TransactionHash(pub [u8; 32]);

impl TransactionHash {
    pub fn new(data: [u8; 32]) -> Self {
        Self(data)
    }

    pub fn none() -> Self {
        Self([0; 32])
    }

    pub fn inner(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for TransactionHash {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        self.0[..].fmt(formatter)
    }
}

// ANCHOR: contractcall
/// A ContractCall is the part of a transaction that executes a certain
/// `contract_id` with `data` as the call's payload.
#[derive(Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct ContractCall {
    /// ID of the contract invoked
    pub contract_id: ContractId,
    /// Call data passed to the contract
    pub data: Vec<u8>,
}
// ANCHOR_END: contractcall

// Avoid showing the data in the debug output since often the calldata is very long.
impl std::fmt::Debug for ContractCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ContractCall(id={:?}", self.contract_id.inner())?;
        let calldata = &self.data;
        if !calldata.is_empty() {
            write!(f, ", function_code={}", calldata[0])?;
        }
        write!(f, ")")
    }
}
