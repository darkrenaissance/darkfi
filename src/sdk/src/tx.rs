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

use std::{
    fmt::{self, Debug},
    str::FromStr,
};

#[cfg(feature = "async")]
use darkfi_serial::async_trait;
use darkfi_serial::{SerialDecodable, SerialEncodable};

use super::{crypto::ContractId, ContractError, GenericResult};
use crate::crypto::{DAO_CONTRACT_ID, DEPLOYOOOR_CONTRACT_ID, MONEY_CONTRACT_ID};

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, SerialEncodable, SerialDecodable)]
// We have to introduce a type rather than using an alias so we can implement Display
pub struct TransactionHash(pub [u8; 32]);

impl TransactionHash {
    pub fn new(data: [u8; 32]) -> Self {
        Self(data)
    }

    pub fn none() -> Self {
        Self([0; 32])
    }

    #[inline]
    pub fn inner(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn as_string(&self) -> String {
        blake3::hash(&self.0).to_string()
    }
}

impl FromStr for TransactionHash {
    type Err = ContractError;

    fn from_str(tx_hash_str: &str) -> GenericResult<Self> {
        let Ok(hash) = blake3::Hash::from_str(tx_hash_str) else {
            return Err(ContractError::HexFmtErr);
        };
        Ok(Self(*hash.as_bytes()))
    }
}

impl fmt::Display for TransactionHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.as_string())
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

impl ContractCall {
    /// Returns true if call is a money fee.
    pub fn is_money_fee(&self) -> bool {
        self.matches_contract_call_type(*MONEY_CONTRACT_ID, 0x00)
    }

    /// Returns true if call is a money genesis mint.
    pub fn is_money_genesis_mint(&self) -> bool {
        self.matches_contract_call_type(*MONEY_CONTRACT_ID, 0x01)
    }

    /// Returns true if call is a money PoW reward.
    pub fn is_money_pow_reward(&self) -> bool {
        self.matches_contract_call_type(*MONEY_CONTRACT_ID, 0x02)
    }

    /// Returns true if call is a money transfer.
    pub fn is_money_transfer(&self) -> bool {
        self.matches_contract_call_type(*MONEY_CONTRACT_ID, 0x03)
    }

    /// Returns true if call is a money over-the-counter swap.
    pub fn is_money_otc_swap(&self) -> bool {
        self.matches_contract_call_type(*MONEY_CONTRACT_ID, 0x04)
    }

    /// Returns true if call is a money token mint authorization.
    pub fn is_money_auth_token_mint(&self) -> bool {
        self.matches_contract_call_type(*MONEY_CONTRACT_ID, 0x05)
    }

    /// Returns true if call is a money token freeze authorization.
    pub fn is_money_auth_token_freeze(&self) -> bool {
        self.matches_contract_call_type(*MONEY_CONTRACT_ID, 0x06)
    }

    /// Returns true if call is a money token mint.
    pub fn is_money_token_mint(&self) -> bool {
        self.matches_contract_call_type(*MONEY_CONTRACT_ID, 0x07)
    }

    /// Returns true if call is a DAO mint.
    pub fn is_dao_mint(&self) -> bool {
        self.matches_contract_call_type(*DAO_CONTRACT_ID, 0x00)
    }

    /// Returns true if call is a DAO proposal.
    pub fn is_dao_propose(&self) -> bool {
        self.matches_contract_call_type(*DAO_CONTRACT_ID, 0x01)
    }

    /// Returns true if call is a DAO vote.
    pub fn is_dao_vote(&self) -> bool {
        self.matches_contract_call_type(*DAO_CONTRACT_ID, 0x02)
    }

    /// Returns true if call is a DAO execution.
    pub fn is_dao_exec(&self) -> bool {
        self.matches_contract_call_type(*DAO_CONTRACT_ID, 0x03)
    }

    /// Returns true if call is a DAO money transfer authorization.
    pub fn is_dao_auth_money_transfer(&self) -> bool {
        self.matches_contract_call_type(*DAO_CONTRACT_ID, 0x04)
    }

    /// Returns true if call is a deployoor deployment.
    pub fn is_deployment(&self) -> bool {
        self.matches_contract_call_type(*DEPLOYOOOR_CONTRACT_ID, 0x00)
    }

    /// Returns true if contract call matches provided contract id and function code.
    pub fn matches_contract_call_type(&self, contract_id: ContractId, func_code: u8) -> bool {
        !self.data.is_empty() && self.contract_id == contract_id && self.data[0] == func_code
    }
}

// Avoid showing the data in the debug output since often the calldata is very long.
impl Debug for ContractCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ContractCall(id={:?}", self.contract_id.inner())?;
        let calldata = &self.data;
        if !calldata.is_empty() {
            write!(f, ", function_code={}", calldata[0])?;
        }
        write!(f, ")")
    }
}
