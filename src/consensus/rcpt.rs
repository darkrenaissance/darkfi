use darkfi_sdk::{
    crypto::{
        keypair::{PublicKey},
        diffie_hellman::{kdf_sapling, sapling_ka_agree},
        pedersen::{pedersen_commitment_base, pedersen_commitment_u64},
        poseidon_hash,
        util::mod_r_p,
        MerkleNode, SecretKey,
    },
    pasta::{arithmetic::CurveAffine, group::Curve, pallas},


};
use halo2_proofs::{arithmetic::Field, circuit::Value};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use log::debug;
use rand::rngs::OsRng;

use darkfi_serial::{Encodable, Decodable, SerialDecodable, SerialEncodable};
use super::constants::{EPOCH_LENGTH};
use crate::{
    crypto::{proof::ProvingKey, Proof},
    zk::{vm::ZkCircuit, vm_stack::Witness},
    zkas::ZkBinary,
    Result, Error,
};
use crypto_api_chachapoly::ChachaPolyIetf;

/// transfered leadcoin is rcpt into two coins,
/// first coin is transfered rcpt coin.
/// second coin is the change returning to sender, or different address.
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct TxRcpt {
    /// rcpt coin nonce
    pub rho: pallas::Base,
    /// rcpt coin commitment opening
    pub opening: pallas::Scalar,
    /// rcpt coin value
    pub value: u64,
}


pub const PLAINTEXT_SIZE: usize = 32 + 32 + 8;
pub const AEAD_TAG_SIZE: usize = 16;
pub const CIPHER_SIZE: usize = PLAINTEXT_SIZE + AEAD_TAG_SIZE;

impl TxRcpt {
    /// encrypt received coin, by recipient public key
    pub fn encrypt(&self, public: &PublicKey) -> EncryptedTxRcpt {
        let ephem_secret = SecretKey::random(&mut OsRng);
        let ephem_public = PublicKey::from_secret(ephem_secret);
        let shared_secret = sapling_ka_agree(&ephem_secret, public);
        let key = kdf_sapling(&shared_secret, &ephem_public);

        let mut input = Vec::new();
        self.encode(&mut input).unwrap();

        let mut ciphertext = [0u8; CIPHER_SIZE];
        assert_eq!(
            ChachaPolyIetf::aead_cipher()
                .seal_to(&mut ciphertext, &input, &[], key.as_ref(), &[0u8; 12])
                .unwrap(),
            CIPHER_SIZE
        );

        EncryptedTxRcpt { ciphertext, ephem_public }
    }
}


#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct EncryptedTxRcpt {
    ciphertext: [u8; CIPHER_SIZE],
    ephem_public: PublicKey,
}

impl EncryptedTxRcpt {
    pub fn decrypt(&self, secret: &SecretKey) -> TxRcpt {
        let shared_secret = sapling_ka_agree(secret, &self.ephem_public);
        let key = kdf_sapling(&shared_secret, &self.ephem_public);

        let mut plaintext = [0; CIPHER_SIZE];
        assert_eq!(
            ChachaPolyIetf::aead_cipher()
                .open_to(&mut plaintext, &self.ciphertext, &[], key.as_ref(), &[0u8; 12])
                .map_err(|_| Error::TxRcptDecryptionError).unwrap(),
            PLAINTEXT_SIZE
        );

        TxRcpt::decode(&plaintext[..]).unwrap()
    }
}
