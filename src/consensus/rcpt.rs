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

use crypto_api_chachapoly::ChachaPolyIetf;
use darkfi_sdk::{
    crypto::{
        diffie_hellman::{kdf_sapling, sapling_ka_agree},
        keypair::PublicKey,
        SecretKey,
    },
    pasta::pallas,
};
use darkfi_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};
use rand::rngs::OsRng;

use crate::Error;

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
                .map_err(|_| Error::TxRcptDecryptionError)
                .unwrap(),
            PLAINTEXT_SIZE
        );

        TxRcpt::decode(&plaintext[..]).unwrap()
    }
}
