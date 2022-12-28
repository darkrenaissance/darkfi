/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use darkfi_serial::{SerialDecodable, SerialEncodable};
use lazy_static::lazy_static;
use pasta_curves::{group::ff::PrimeField, pallas};

use super::{poseidon_hash, PublicKey, SecretKey};
use crate::error::ContractError;

lazy_static! {
    // The idea here is that 0 is not a valid x coordinate for any pallas point,
    // therefore a signature cannot be produced for such IDs. This allows us to
    // avoid hardcoding contract IDs for arbitrary contract deployments, because
    // the contracts with 0 as their x coordinate can never have a valid signature.

    /// Native DARK token ID
    pub static ref DARK_TOKEN_ID: TokenId =
        TokenId::from(poseidon_hash([pallas::Base::zero(), pallas::Base::from(42)]));
}

/// TokenId represents an on-chain identifier for a certain token.
#[derive(Copy, Clone, Debug, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct TokenId(pallas::Base);

impl TokenId {
    /// Derives a `TokenId` given a `SecretKey` (mint authority)
    pub fn derive(mint_authority: SecretKey) -> Self {
        let public_key = PublicKey::from_secret(mint_authority);
        let (x, y) = public_key.xy();
        let hash = poseidon_hash::<2>([x, y]);
        Self(hash)
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
}

impl From<pallas::Base> for TokenId {
    fn from(x: pallas::Base) -> Self {
        Self(x)
    }
}

impl core::fmt::Display for TokenId {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        // Base58 encoding
        let tokenid: String = bs58::encode(self.0.to_repr()).into_string();
        write!(f, "{}", tokenid)
    }
}

impl TryFrom<&str> for TokenId {
    type Error = ContractError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let bytes: [u8; 32] = match bs58::decode(s).into_vec() {
            Ok(v) => {
                if v.len() != 32 {
                    return Err(ContractError::IoError(
                        "Decoded bs58 string for TokenId is not 32 bytes long".to_string(),
                    ))
                }

                v.try_into().unwrap()
            }
            Err(e) => {
                return Err(ContractError::IoError(format!(
                    "Failed to decode bs58 for TokenId: {}",
                    e
                )))
            }
        };

        match pallas::Base::from_repr(bytes).into() {
            Some(v) => Ok(Self(v)),
            None => Err(ContractError::IoError("Bytes for TokenId are noncanonical".to_string())),
        }
    }
}
