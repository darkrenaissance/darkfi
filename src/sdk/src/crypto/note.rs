/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use chacha20poly1305::{AeadInPlace, ChaCha20Poly1305, KeyInit};
use darkfi_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};
use pasta_curves::{group::ff::Field, pallas};
use rand_core::{CryptoRng, RngCore};

#[cfg(feature = "async")]
use darkfi_serial::async_trait;

use super::{diffie_hellman, poseidon_hash, util::fp_mod_fv, PublicKey, SecretKey};
use crate::error::ContractError;

/// AEAD tag length in bytes
pub const AEAD_TAG_SIZE: usize = 16;

/// An encrypted note using Diffie-Hellman and ChaCha20Poly1305
#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct AeadEncryptedNote {
    pub ciphertext: Vec<u8>,
    pub ephem_public: PublicKey,
}

impl AeadEncryptedNote {
    pub fn encrypt(
        note: &impl Encodable,
        public: &PublicKey,
        rng: &mut (impl CryptoRng + RngCore),
    ) -> Result<Self, ContractError> {
        let ephem_secret = SecretKey::random(rng);
        let ephem_public = PublicKey::from_secret(ephem_secret);
        let shared_secret = diffie_hellman::sapling_ka_agree(&ephem_secret, public);
        let key = diffie_hellman::kdf_sapling(&shared_secret, &ephem_public);

        let mut input = Vec::new();
        note.encode(&mut input)?;
        let input_len = input.len();

        let mut ciphertext = vec![0_u8; input_len + AEAD_TAG_SIZE];
        ciphertext[..input_len].copy_from_slice(&input);

        ChaCha20Poly1305::new(key.as_ref().into())
            .encrypt_in_place([0u8; 12][..].into(), &[], &mut ciphertext)
            .unwrap();

        Ok(Self { ciphertext, ephem_public })
    }

    pub fn decrypt<D: Decodable>(&self, secret: &SecretKey) -> Result<D, ContractError> {
        let shared_secret = diffie_hellman::sapling_ka_agree(secret, &self.ephem_public);
        let key = diffie_hellman::kdf_sapling(&shared_secret, &self.ephem_public);

        let ct_len = self.ciphertext.len();
        let mut plaintext = vec![0_u8; ct_len];
        plaintext.copy_from_slice(&self.ciphertext);

        match ChaCha20Poly1305::new(key.as_ref().into()).decrypt_in_place(
            [0u8; 12][..].into(),
            &[],
            &mut plaintext,
        ) {
            Ok(()) => Ok(D::decode(&plaintext[..ct_len - AEAD_TAG_SIZE])?),
            Err(e) => Err(ContractError::IoError(format!("Note decrypt failed: {}", e))),
        }
    }
}

/// An encrypted note using an ElGamal scheme verifiable in ZK.
///
/// **WARNING:**
/// Without ZK, there is no authentication of the ciphertexts so these should
/// not be used without a corresponding ZK proof.
#[derive(Debug, Copy, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct ElGamalEncryptedNote<const N: usize> {
    /// The values encrypted with the derived shared secret using Diffie-Hellman
    pub encrypted_values: [pallas::Base; N],
    /// The ephemeral public key used for Diffie-Hellman key derivation
    pub ephem_public: PublicKey,
}

impl<const N: usize> ElGamalEncryptedNote<N> {
    /// Encrypt given values to the given `PublicKey` using a `SecretKey` for Diffie-Hellman
    ///
    /// Note that this does not do any message authentication.
    /// This means that alterations of the ciphertexts lead to the same alterations
    /// on the plaintexts.
    pub fn encrypt_unsafe(
        values: [pallas::Base; N],
        ephem_secret: &SecretKey,
        public: &PublicKey,
    ) -> Self {
        // Derive shared secret using DH
        let ephem_public = PublicKey::from_secret(*ephem_secret);
        let (ss_x, ss_y) = PublicKey::from(public.inner() * fp_mod_fv(ephem_secret.inner())).xy();
        let shared_secret = poseidon_hash([ss_x, ss_y]);

        // Derive the blinds using the shared secret and incremental nonces
        let mut blinds = [pallas::Base::ZERO; N];
        for (i, item) in blinds.iter_mut().enumerate().take(N) {
            *item = poseidon_hash([shared_secret, pallas::Base::from(i as u64 + 1)]);
        }

        // Encrypt the values
        let mut encrypted_values = [pallas::Base::ZERO; N];
        for i in 0..N {
            encrypted_values[i] = values[i] + blinds[i];
        }

        Self { encrypted_values, ephem_public }
    }

    /// Decrypt the `ElGamalEncryptedNote` using a `SecretKey` for shared secret derivation
    /// using Diffie-Hellman
    ///
    /// Note that this does not do any message authentication.
    /// This means that alterations of the ciphertexts lead to the same alterations
    /// on the plaintexts.
    pub fn decrypt_unsafe(&self, secret: &SecretKey) -> [pallas::Base; N] {
        // Derive shared secret using DH
        let (ss_x, ss_y) =
            PublicKey::from(self.ephem_public.inner() * fp_mod_fv(secret.inner())).xy();
        let shared_secret = poseidon_hash([ss_x, ss_y]);

        let mut blinds = [pallas::Base::ZERO; N];
        for (i, item) in blinds.iter_mut().enumerate().take(N) {
            *item = poseidon_hash([shared_secret, pallas::Base::from(i as u64 + 1)]);
        }

        let mut decrypted_values = [pallas::Base::ZERO; N];
        for i in 0..N {
            decrypted_values[i] = self.encrypted_values[i] - blinds[i];
        }

        decrypted_values
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::Keypair;

    use halo2_proofs::arithmetic::Field;
    use rand::rngs::OsRng;

    #[test]
    fn test_aead_note() {
        let plaintext = "gm world";
        let keypair = Keypair::random(&mut OsRng);

        let encrypted_note =
            AeadEncryptedNote::encrypt(&plaintext, &keypair.public, &mut OsRng).unwrap();

        let plaintext2: String = encrypted_note.decrypt(&keypair.secret).unwrap();

        assert_eq!(plaintext, plaintext2);
    }

    #[test]
    fn test_elgamal_note() {
        const N_MSGS: usize = 10;

        let plain_values = [pallas::Base::random(&mut OsRng); N_MSGS];
        let keypair = Keypair::random(&mut OsRng);
        let ephem_secret = SecretKey::random(&mut OsRng);

        let encrypted_note =
            ElGamalEncryptedNote::encrypt_unsafe(plain_values, &ephem_secret, &keypair.public);

        let decrypted_values = encrypted_note.decrypt_unsafe(&keypair.secret);

        assert_eq!(plain_values, decrypted_values);
    }
}
