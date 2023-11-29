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

use std::convert::TryInto;

use pasta_curves::group::ff::PrimeField;
use sha2::{Digest, Sha256};

use crate::{
    crypto::{PublicKey, SecretKey},
    error::ContractError,
    pasta::pallas,
};
use std::fmt;

fn double_sha256(input: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let first = hasher.finalize();

    let mut hasher = Sha256::new();
    hasher.update(first);
    hasher.finalize().into()
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShareAddress {
    pub raw: [u8; 32],
    pub prefix: ShareAddressType,
    full_address: Option<bool>,
}

#[derive(Clone, Debug, Copy, PartialEq)]
pub enum ShareAddressType {
    SecretKey = 1,
    PublicKey = 2,
    DaoBulla = 3,
}

/// Wallet import format <https://en.bitcoin.it/wiki/Wallet_import_format>
/// Encodes pallas-curve base field secret key to base58 string.
impl ShareAddress {
    /// Initialize `ShareAddress` from `pallas::Base`
    pub fn from_field(
        field_element: &pallas::Base,
        prefix: ShareAddressType,
        with_checksum: Option<bool>,
    ) -> Self {
        // address to bytes
        let address: [u8; 32] = field_element.to_repr();
        // address prefix
        ShareAddress { raw: address, prefix, full_address: with_checksum }
    }
    /// concatenate prefix with raw bytes
    pub fn extended_address(&self) -> [u8; 33] {
        // create extended address
        let mut extended_addrs: [u8; 33] = [0; 33];
        let (extended_addrs_prefix, extended_addrs_main) = extended_addrs.split_at_mut(1);
        extended_addrs_prefix.copy_from_slice(&[self.prefix as u8]);
        extended_addrs_main.copy_from_slice(&self.raw);
        extended_addrs
    }
    /// extended address checksum
    pub fn checksum(&self) -> [u8; 4] {
        let extended_addrs: [u8; 33] = self.extended_address();
        // extended address checksum
        let checksum: [u8; 4] = double_sha256(&extended_addrs)[0..4].try_into().unwrap();
        checksum
    }
}

impl fmt::Display for ShareAddress {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let extended_addrs = self.extended_address();
        match self.full_address {
            Some(_) => {
                let checksum = self.checksum();
                let mut full_address: [u8; 37] = [0; 37];
                let (full_address_left, full_address_right) =
                    full_address.split_at_mut(extended_addrs.len());
                full_address_left.copy_from_slice(&extended_addrs);
                full_address_right.copy_from_slice(&checksum);
                let base58 = bs58::encode(full_address).into_string();
                fmt.write_str(&base58)?;
            }
            None => {
                let base58 = bs58::encode(extended_addrs).into_string();
                fmt.write_str(&base58)?;
            }
        }
        Ok(())
    }
}

/// convert decode `ShareAddress` string
impl TryFrom<std::string::String> for ShareAddress {
    type Error = String;
    fn try_from(share_address_str: String) -> Result<Self, Self::Error> {
        let full_address: Vec<u8> = bs58::decode(share_address_str).into_vec().unwrap();

        if full_address.len() != 37 && full_address.len() != 33 {
            return Err("invalid share address".to_string())
        }
        let is_full_address = if full_address.len() == 37 { Some(true) } else { None };
        // extract prefix
        let prefix: u8 = full_address[0];
        // get address
        let addrs: [u8; 32] = full_address[1..33].try_into().expect("slice with incorrect length");
        // get extended address
        let extended_addrs: [u8; 33] =
            full_address[0..33].try_into().expect("slice with incorrect length");
        if full_address.len() == 37 {
            // get checksum
            let wif_checksum: [u8; 4] =
                full_address[33..37].try_into().expect("slice with incorrect length");
            // validate checksum
            let checksum: [u8; 4] = double_sha256(&extended_addrs)[0..4].try_into().unwrap();
            assert!(wif_checksum == checksum);
        }
        if prefix != ShareAddressType::SecretKey as u8 {
            return Err("wrong prefix".to_string())
        }
        Ok(ShareAddress {
            raw: addrs,
            prefix: ShareAddressType::SecretKey,
            full_address: is_full_address,
        })
    }
}

impl TryInto<SecretKey> for ShareAddress {
    type Error = String;
    fn try_into(self) -> Result<SecretKey, Self::Error> {
        if self.prefix != ShareAddressType::SecretKey {
            return Err("wrong prefix".to_string())
        }
        let sk: Result<SecretKey, ContractError> = SecretKey::from_bytes(self.raw);
        Ok(sk.unwrap())
    }
}

impl TryInto<PublicKey> for ShareAddress {
    type Error = String;
    fn try_into(self) -> Result<PublicKey, Self::Error> {
        if self.prefix != ShareAddressType::PublicKey {
            return Err("wrong prefix".to_string())
        }
        let sk: Result<PublicKey, ContractError> = PublicKey::from_bytes(self.raw);
        Ok(sk.unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sk_share_address() {
        let sk_bytes: [u8; 32] = [0; 32];
        let sk = SecretKey::from_bytes(sk_bytes).unwrap();
        let share_address =
            ShareAddress::from_field(&sk.inner(), ShareAddressType::SecretKey, Some(true));
        let share_address_str = share_address.to_string();
        let share_address_rhs = match ShareAddress::try_from(share_address_str) {
            Err(why) => panic!("{:?}", why),
            Ok(value) => value,
        };
        assert!(share_address == share_address_rhs);
    }
    #[test]
    fn test_sk_mini_share_addres() {
        let sk_bytes: [u8; 32] = [0; 32];
        let sk = SecretKey::from_bytes(sk_bytes).unwrap();
        let mini_share_address =
            ShareAddress::from_field(&sk.inner(), ShareAddressType::SecretKey, None);
        let share_address_str = mini_share_address.to_string();
        let share_address_rhs = match ShareAddress::try_from(share_address_str) {
            Err(why) => panic!("{:?}", why),
            Ok(value) => value,
        };
        assert!(mini_share_address == share_address_rhs);
    }
    #[test]
    fn test_try_from_str() {
        let res = ShareAddress::try_from("abc".to_string());
        assert!(res.is_err());
    }
    #[test]
    fn test_sk_tryfrom() {
        let sk_bytes: [u8; 32] = [0; 32];
        let sk = SecretKey::from_bytes(sk_bytes).unwrap();
        let share_address =
            ShareAddress::from_field(&sk.inner(), ShareAddressType::SecretKey, Some(true));
        let sk_rhs = match share_address.try_into() {
            Err(why) => panic!("{:?}", why),
            Ok(value) => value,
        };
        assert!(sk == sk_rhs);
    }
}
