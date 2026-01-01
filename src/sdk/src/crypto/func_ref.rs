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

use std::str::FromStr;

#[cfg(feature = "async")]
use darkfi_serial::async_trait;
use darkfi_serial::{SerialDecodable, SerialEncodable};
use pasta_curves::pallas;

use super::{pasta_prelude::*, poseidon_hash, ContractId};
use crate::{fp_from_bs58, fp_to_bs58, ty_from_fp, ContractError};

pub type FunctionCode = u8;

#[derive(Copy, Clone, Debug, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct FuncRef {
    pub contract_id: ContractId,
    pub func_code: FunctionCode,
}

impl FuncRef {
    pub fn to_func_id(&self) -> FuncId {
        let func_id =
            poseidon_hash([self.contract_id.inner(), pallas::Base::from(self.func_code as u64)]);
        FuncId(func_id)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct FuncId(pallas::Base);

impl FuncId {
    pub fn none() -> Self {
        Self(pallas::Base::ZERO)
    }

    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    /// Create a `FuncId` object from given bytes, erroring if the
    /// input bytes are noncanonical.
    pub fn from_bytes(x: [u8; 32]) -> Result<Self, ContractError> {
        match pallas::Base::from_repr(x).into() {
            Some(v) => Ok(Self(v)),
            None => {
                Err(ContractError::IoError("Failed to instantiate FuncId from bytes".to_string()))
            }
        }
    }

    /// Convert the `FuncId` type into 32 raw bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }
}

fp_from_bs58!(FuncId);
fp_to_bs58!(FuncId);
ty_from_fp!(FuncId);
