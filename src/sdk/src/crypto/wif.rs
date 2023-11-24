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
    crypto::{
        constants::{MAINNET_ADDRS_PREFIX, TESTNET_ADDRS_PREFIX},
        SecretKey,
    },
    error::ContractError,
};

fn double_sha256(input: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let first = hasher.finalize();

    let mut hasher = Sha256::new();
    hasher.update(first);
    hasher.finalize().into()
}

#[derive(Clone, Debug)]
pub struct Wif(String);

/// Wallet import format <https://en.bitcoin.it/wiki/Wallet_import_format>
/// Encodes pallas-curve base field secret key to base58 string.
impl Wif {
    /// Get inner wrapped object
    pub fn inner(&self) -> String {
        self.0.clone()
    }
}

/// Convert `SecretKey` to wallet import format `Wif`
impl From<&SecretKey> for Wif {
    /// Initialize WIF from `SecretKey`
    fn from(secretkey: &SecretKey) -> Self {
        // address to bytes
        let address: [u8; 32] = secretkey.inner().to_repr();
        // address prefix
        // TODO implement is_mainnet()  in `SecretKey`
        // let is mainnet_prefix = secretkey.is_mainnet();
        let is_mainnet_prefix = true;
        let prefix: [u8; 1] =
            if is_mainnet_prefix { MAINNET_ADDRS_PREFIX } else { TESTNET_ADDRS_PREFIX };
        // create extended address
        let mut extended_addrs: [u8; 33] = [0; 33];
        let (extended_addrs_prefix, extended_addrs_main) = extended_addrs.split_at_mut(1);
        extended_addrs_prefix.copy_from_slice(&prefix);
        extended_addrs_main.copy_from_slice(&address);
        // extended address checksum
        let checksum: [u8; 4] = double_sha256(&extended_addrs)[0..4].try_into().unwrap();
        let mut full_address: [u8; 37] = [0; 37];
        let (full_address_left, full_address_right) =
            full_address.split_at_mut(extended_addrs.len());
        full_address_left.copy_from_slice(&extended_addrs);
        full_address_right.copy_from_slice(&checksum);
        Wif(bs58::encode(full_address).into_string())
    }
}

/// convert wallet import format `WIF` to `SecretKey`
impl From<Wif> for SecretKey {
    fn from(wif: Wif) -> Self {
        let full_address: Vec<u8> = bs58::decode(wif.0).into_vec().unwrap();
        // extract prefix
        // TODO set secret key type mainnet/testnet
        //let prefix : [u8;1] = full_address[0..1].try_into().expect("slice with incorrect length");
        // get address
        let addrs: [u8; 32] = full_address[1..33].try_into().expect("slice with incorrect length");
        // get extended address
        let extended_addrs: [u8; 33] =
            full_address[0..33].try_into().expect("slice with incorrect length");
        // get checksum
        let wif_checksum: [u8; 4] =
            full_address[33..37].try_into().expect("slice with incorrect length");
        // validate checksum
        let checksum: [u8; 4] = double_sha256(&extended_addrs)[0..4].try_into().unwrap();
        assert!(wif_checksum == checksum);
        let sk: Result<SecretKey, ContractError> = SecretKey::from_bytes(addrs);
        sk.unwrap()
    }
}

/// attempt to convert WIF into `SecretKey`.
impl TryInto<SecretKey> for String {
    type Error = String;
    fn try_into(self) -> Result<SecretKey, Self::Error> {
        let full_address: Vec<u8> = bs58::decode(self).into_vec().unwrap();
        // extract prefix
        // TODO set secret key type mainnet/testnet
        //let prefix : [u8;1] = full_address[0..1].try_into().expect("slice with incorrect length");
        // get address
        let addrs: [u8; 32] = full_address[1..33].try_into().expect("slice with incorrect length");
        // get extended address
        let extended_addrs: [u8; 33] =
            full_address[0..33].try_into().expect("slice with incorrect length");
        // get checksum
        let wif_checksum: [u8; 4] =
            full_address[33..37].try_into().expect("slice with incorrect length");
        // validate checksum
        let checksum: [u8; 4] = double_sha256(&extended_addrs)[0..4].try_into().unwrap();
        assert!(wif_checksum == checksum);
        let sk: Result<SecretKey, ContractError> = SecretKey::from_bytes(addrs);
        Ok(sk.unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sk_towif() {
        let sk_bytes: [u8; 32] = [0; 32];
        let _sk_str = std::str::from_utf8(&sk_bytes).unwrap();
        let sk = SecretKey::from_bytes(sk_bytes).unwrap();
        let wif = Wif::from(&sk);

        let sk_res = match wif.try_into() {
            Err(why) => panic!("{:?}", why),
            Ok(value) => value,
        };
        assert!(sk == sk_res);
    }
}
