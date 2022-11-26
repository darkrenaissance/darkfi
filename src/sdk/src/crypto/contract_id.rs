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

use super::{poseidon_hash, PublicKey, SecretKey};
use crate::error::ContractError;

/// ContractId represents an on-chain identifier for a certain smart contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct ContractId(pallas::Base);

impl ContractId {
    /// Derive a contract ID from a `SecretKey` (deploy key)
    pub fn derive(deploy_key: SecretKey) -> Self {
        let public_key = PublicKey::from_secret(deploy_key);
        let (x, y) = public_key.xy();
        let hash = poseidon_hash::<2>([x, y]);
        Self(hash)
    }

    /// Get the inner `pallas::Base` element.
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    /// Create a `ContractId` object from given bytes.
    pub fn from_bytes(x: [u8; 32]) -> Result<Self, ContractError> {
        match pallas::Base::from_repr(x).into() {
            Some(v) => Ok(Self(v)),
            None => Err(ContractError::IoError(
                "Failed to instantiate ContractId from bytes".to_string(),
            )),
        }
    }

    /// Convert a `ContractId` object to its byte representation
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }

    /// `blake3(self || tree_name)` is used in datbases to have a
    /// fixed-size name for a contract's state db.
    pub fn hash_state_id(&self, tree_name: &str) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&serialize(self));
        hasher.update(tree_name.as_bytes());
        let id = hasher.finalize();
        *id.as_bytes()
    }
}

impl From<pallas::Base> for ContractId {
    fn from(x: pallas::Base) -> Self {
        Self(x)
    }
}

impl core::fmt::Display for ContractId {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        // Base58 encoding
        let contractid: String = bs58::encode(self.0.to_repr()).into_string();
        write!(f, "{}", contractid)
    }
}

impl TryFrom<&str> for ContractId {
    type Error = ContractError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let bytes: [u8; 32] = match bs58::decode(s).into_vec() {
            Ok(v) => {
                if v.len() != 32 {
                    return Err(ContractError::IoError(
                        "Decoded bs58 string for ContractId is not 32 bytes long".to_string(),
                    ))
                }

                v.try_into().unwrap()
            }
            Err(e) => {
                return Err(ContractError::IoError(format!(
                    "Failed to decode bs58 for ContractId: {}",
                    e
                )))
            }
        };

        match pallas::Base::from_repr(bytes).into() {
            Some(v) => Ok(Self(v)),
            None => {
                Err(ContractError::IoError("Bytes for ContractId are noncanonical".to_string()))
            }
        }
    }
}
