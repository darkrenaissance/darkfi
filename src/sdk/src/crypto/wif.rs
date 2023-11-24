use crate::{
    crypto::{
        constants::{MAINNET_ADDRS_PREFIX, TESTNET_ADDRS_PREFIX},
        SecretKey,
    },
    error::ContractError,
};
use bs58;
use pasta_curves::group::ff::PrimeField;
use sha256;
use std::convert::TryInto;

#[derive(Clone, Debug)]
pub struct WIF(String);

/// wallet import format https://en.bitcoin.it/wiki/Wallet_import_format
/// encodes pallas-curve base private key to base58 string.
impl WIF {
    /// Get inner wrapped object
    pub fn inner(&self) -> String {
        self.0.clone()
    }
}

/// convert `SecretKey` to wallet import format `WIF`
impl From<SecretKey> for WIF {
    /// Initialize WIF from `SecretKey`
    fn from(secretkey: SecretKey) -> Self {
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
        let checksum: [u8; 4] = {
            let first_digest: String = sha256::digest_bytes(&extended_addrs);
            let second_digest: &[u8] = &sha256::digest(first_digest).into_bytes();
            [second_digest[0], second_digest[1], second_digest[2], second_digest[3]]
        };
        let mut full_address: [u8; 37] = [0; 37];
        let (full_address_left, full_address_right) =
            full_address.split_at_mut(extended_addrs.len());
        full_address_left.copy_from_slice(&extended_addrs);
        full_address_right.copy_from_slice(&checksum);
        WIF(bs58::encode(full_address).into_string())
    }
}

/// convert wallet import format `WIF` to `SecretKey`
impl From<WIF> for SecretKey {
    fn from(wif: WIF) -> Self {
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
        let checksum: [u8; 4] = {
            let first_digest: String = sha256::digest_bytes(&extended_addrs);
            let second_digest: &[u8] = &sha256::digest(first_digest).into_bytes();
            [second_digest[0], second_digest[1], second_digest[2], second_digest[3]]
        };
        assert!(wif_checksum == checksum);
        let sk: Result<SecretKey, ContractError> = SecretKey::from_bytes(addrs).into();
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
        let checksum: [u8; 4] = {
            let first_digest: String = sha256::digest_bytes(&extended_addrs);
            let second_digest: &[u8] = &sha256::digest(first_digest).into_bytes();
            [second_digest[0], second_digest[1], second_digest[2], second_digest[3]]
        };
        assert!(wif_checksum == checksum);
        let sk: Result<SecretKey, ContractError> = SecretKey::from_bytes(addrs).into();
        Ok(sk.unwrap())
    }
}
