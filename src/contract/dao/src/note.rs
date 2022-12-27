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

use chacha20poly1305::{AeadInPlace, ChaCha20Poly1305, KeyInit};
use darkfi_sdk::crypto::{
    diffie_hellman::{kdf_sapling, sapling_ka_agree},
    PublicKey, SecretKey,
};
use darkfi_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};
use rand::rngs::OsRng;

use darkfi::{Error, Result};

pub const AEAD_TAG_SIZE: usize = 16;

pub fn encrypt<T: Encodable>(note: &T, public: &PublicKey) -> Result<EncryptedNote2> {
    let ephem_secret = SecretKey::random(&mut OsRng);
    let ephem_public = PublicKey::from_secret(ephem_secret);
    let shared_secret = sapling_ka_agree(&ephem_secret, public);
    let key = kdf_sapling(&shared_secret, &ephem_public);

    let mut input = Vec::new();
    note.encode(&mut input)?;
    let input_len = input.len();

    let mut ciphertext = vec![0_u8; input_len + AEAD_TAG_SIZE];
    ciphertext[..input_len].copy_from_slice(&input);

    ChaCha20Poly1305::new(key.as_ref().into())
        .encrypt_in_place([0u8; 12][..].into(), &[], &mut ciphertext)
        .unwrap();

    Ok(EncryptedNote2 { ciphertext, ephem_public })
}

#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct EncryptedNote2 {
    pub ciphertext: Vec<u8>,
    pub ephem_public: PublicKey,
}

impl EncryptedNote2 {
    pub fn decrypt<T: Decodable>(&self, secret: &SecretKey) -> Result<T> {
        let shared_secret = sapling_ka_agree(secret, &self.ephem_public);
        let key = kdf_sapling(&shared_secret, &self.ephem_public);

        let ciphertext_len = self.ciphertext.len();
        let mut plaintext = vec![0_u8; ciphertext_len];
        plaintext.copy_from_slice(&self.ciphertext);

        match ChaCha20Poly1305::new(key.as_ref().into()).decrypt_in_place(
            [0u8; 12][..].into(),
            &[],
            &mut plaintext,
        ) {
            Ok(()) => {
                Ok(T::decode(&plaintext[..ciphertext_len - AEAD_TAG_SIZE]).map_err(Error::from)?)
            }
            Err(e) => Err(Error::NoteDecryptionFailed(e.to_string())),
        }
    }
}

// TODO: This test is broken:
//       1. no darkfi::crypto::type
//       2. SerialDecodable cannot infer type for type parameter `D`
/*
#[cfg(test)]
mod tests {
    use super::*;
    use darkfi::crypto::types::{DrkCoinBlind, DrkSerial, DrkValueBlind};
    use darkfi_sdk::{
        crypto::{Keypair, TokenId},
        pasta::{group::ff::Field, pallas},
    };

    #[test]
    fn test_note_encdec() {
        #[derive(SerialEncodable, SerialDecodable)]
        struct MyNote {
            serial: DrkSerial,
            value: u64,
            token_id: TokenId,
            coin_blind: DrkCoinBlind,
            value_blind: DrkValueBlind,
            token_blind: DrkValueBlind,
            memo: Vec<u8>,
        }
        let note = MyNote {
            serial: DrkSerial::random(&mut OsRng),
            value: 110,
            token_id: TokenId::from(pallas::Base::random(&mut OsRng)),
            coin_blind: DrkCoinBlind::random(&mut OsRng),
            value_blind: DrkValueBlind::random(&mut OsRng),
            token_blind: DrkValueBlind::random(&mut OsRng),
            memo: vec![32, 223, 231, 3, 1, 1],
        };

        let keypair = Keypair::random(&mut OsRng);

        let encrypted_note = encrypt(&note, &keypair.public).unwrap();
        let note2: MyNote = encrypted_note.decrypt(&keypair.secret).unwrap();
        assert_eq!(note.value, note2.value);
        assert_eq!(note.token_id, note2.token_id);
        assert_eq!(note.token_blind, note2.token_blind);
        assert_eq!(note.memo, note2.memo);
    }
}
*/
