use group::GroupEncoding;
use sha2::Digest;

use crate::types::*;

#[derive(Clone, Debug)]
pub struct Address {
    pub raw: DrkPublicKey,
    pub pkh: String,
}

impl Address {
    pub fn new(raw: DrkPublicKey) -> Self {
        let pkh = Self::pkh_address(&raw);

        Address { raw, pkh }
    }

    fn get_hash(raw: &DrkPublicKey) -> Vec<u8> {
        // sha256
        let mut hasher = sha2::Sha256::new();
        hasher.update(raw.to_bytes());
        let hash = hasher.finalize();

        // ripemd160
        let mut hasher = ripemd160::Ripemd160::new();
        hasher.update(hash.to_vec());
        let hash = hasher.finalize();

        hash.to_vec()
    }

    pub fn pkh_address(raw: &DrkPublicKey) -> String {
        let mut hash = Self::get_hash(raw);

        let mut payload = vec![];

        // add version
        payload.push(0x00 as u8);

        // add public key hash
        payload.append(&mut hash);

        // hash the payload + version
        let mut hasher = sha2::Sha256::new();
        hasher.update(payload.clone());
        let payload_hash = hasher.finalize().to_vec();

        payload.append(&mut payload_hash[0..4].to_vec());

        // base56 encoding
        let address: String = bs58::encode(payload).into_string();

        address
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.pkh)
    }
}
