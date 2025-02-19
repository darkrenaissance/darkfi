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

use darkfi_sdk::{
    crypto::{
        constants::DRK_TOKEN_ID_PERSONALIZATION, pasta_prelude::PrimeField, util::hash_to_base,
    },
    error::ContractError,
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};
use lazy_static::lazy_static;

#[cfg(feature = "client")]
use darkfi_serial::async_trait;

use super::poseidon_hash;

lazy_static! {
    // Is this even needed? Not used elsewhere except here.
    /// Derivation prefix for `TokenId`
    pub static ref TOKEN_ID_PREFIX: pallas::Base = pallas::Base::from(69);

    /// Native DARK token ID.
    /// It does not correspond to any real commitment since we only rely on this value as
    /// a constant.
    pub static ref DARK_TOKEN_ID: TokenId = TokenId(hash_to_base(&[0x69], &[DRK_TOKEN_ID_PERSONALIZATION]));
}

/// TokenId represents an on-chain identifier for a certain token.
#[derive(Copy, Clone, Debug, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct TokenId(pallas::Base);

impl TokenId {
    /// Derives a `TokenId` from provided function id,
    /// user data and blind.
    pub fn derive_from(
        func_id: pallas::Base,
        user_data: pallas::Base,
        blind: pallas::Base,
    ) -> Self {
        let token_id = poseidon_hash([func_id, user_data, blind]);
        Self(token_id)
    }

    /// Get the inner `pallas::Base` element.
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    /// Create a `TokenId` object from given bytes, erroring if the input
    /// bytes are noncanonical.
    pub fn from_bytes(x: [u8; 32]) -> Result<Self, ContractError> {
        match pallas::Base::from_repr(x).into() {
            Some(v) => Ok(Self(v)),
            None => {
                Err(ContractError::IoError("Failed to instantiate TokenId from bytes".to_string()))
            }
        }
    }

    /// Convert the `TokenId` type into 32 raw bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }
}

use core::str::FromStr;
darkfi_sdk::fp_from_bs58!(TokenId);
darkfi_sdk::fp_to_bs58!(TokenId);
darkfi_sdk::ty_from_fp!(TokenId);
