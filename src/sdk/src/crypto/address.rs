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

// TODO: This module should use blake3, and be a bit more robust with a
//       more clear and consistent API

use core::str::FromStr;

use darkfi_serial::{serialize, SerialDecodable, SerialEncodable};
use sha2::Digest;

use super::PublicKey;
use crate::error::ContractError;

enum AddressType {
    Payment = 0,
}

#[derive(
    Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, SerialEncodable, SerialDecodable,
)]
pub struct Address([u8; 37]);

impl Address {
    pub fn inner(&self) -> [u8; 37] {
        self.0
    }

    fn is_valid_address(address: Vec<u8>) -> bool {
        if address.starts_with(&[AddressType::Payment as u8]) && address.len() == 37 {
            // hash the version + publickey to check the checksum
            let mut hasher = sha2::Sha256::new();
            hasher.update(&address[..33]);
            let payload_hash = hasher.finalize().to_vec();

            payload_hash[..4] == address[33..]
        } else {
            false
        }
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        // base58 encoding
        let address: String = bs58::encode(self.0).into_string();
        write!(f, "{}", address)
    }
}

impl FromStr for Address {
    type Err = ContractError;

    fn from_str(address: &str) -> Result<Self, Self::Err> {
        let bytes = bs58::decode(&address).into_vec();

        if let Ok(v) = bytes {
            if Self::is_valid_address(v.clone()) {
                let mut bytes_arr = [0u8; 37];
                bytes_arr.copy_from_slice(v.as_slice());
                return Ok(Self(bytes_arr))
            }
        }

        Err(ContractError::IoError("Invalid address".to_string()))
    }
}

impl From<PublicKey> for Address {
    fn from(public_key: PublicKey) -> Self {
        let mut public_key = serialize(&public_key);

        // add version
        let mut address = vec![AddressType::Payment as u8];

        // add public key
        address.append(&mut public_key);

        // hash the version + publickey
        let mut hasher = sha2::Sha256::new();
        hasher.update(address.clone());
        let payload_hash = hasher.finalize().to_vec();

        // add the 4 first bytes from the hash as checksum
        address.append(&mut payload_hash[..4].to_vec());

        let mut payment_address = [0u8; 37];
        payment_address.copy_from_slice(address.as_slice());

        Self(payment_address)
    }
}

/* FIXME:
#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::Keypair;
    use rand::rngs::OsRng;

    #[test]
    fn test_address() -> Result<(), ContractError> {
        // from/to PublicKey
        let keypair = Keypair::random(&mut OsRng);
        let address = Address::from(keypair.public);
        assert_eq!(keypair.public, PublicKey::try_from(address)?);

        // from/to string
        let address_str = address.to_string();
        let from_str = Address::from_str(&address_str)?;
        assert_eq!(from_str, address);

        Ok(())
    }
}
*/
