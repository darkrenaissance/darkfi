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
#[cfg(feature = "async")]
use darkfi_serial::async_trait;
use darkfi_serial::{SerialDecodable, SerialEncodable};
use pasta_curves::pallas;

use super::{pasta_prelude::*, poseidon_hash, ContractId};

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
}

impl From<pallas::Base> for FuncId {
    fn from(func_id: pallas::Base) -> Self {
        Self(func_id)
    }
}
