use chacha20poly1305::{AeadInPlace, ChaCha20Poly1305, KeyInit};
use darkfi_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};
use rand::rngs::OsRng;

use crate::{
    crypto::{
        diffie_hellman::{kdf_sapling, sapling_ka_agree},
        keypair::{PublicKey, SecretKey},
        types::{DrkCoinBlind, DrkSerial, DrkTokenId, DrkValueBlind},
    },
    Error, Result,
};

pub const AEAD_TAG_SIZE: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Note {
    pub serial: DrkSerial,
    pub value: u64,
    pub token_id: DrkTokenId,
    pub coin_blind: DrkCoinBlind,
    pub value_blind: DrkValueBlind,
    pub token_blind: DrkValueBlind,
    pub memo: Vec<u8>,
}

impl Note {
    pub fn encrypt(&self, public: &PublicKey) -> Result<EncryptedNote> {
        let ephem_secret = SecretKey::random(&mut OsRng);
        let ephem_public = PublicKey::from_secret(ephem_secret);
        let shared_secret = sapling_ka_agree(&ephem_secret, public);
        let key = kdf_sapling(&shared_secret, &ephem_public);

        let mut input = Vec::new();
        self.encode(&mut input)?;
        let input_len = input.len();

        let mut ciphertext = vec![0_u8; input_len + AEAD_TAG_SIZE];
        ciphertext[..input_len].copy_from_slice(&input);

        let tag = ChaCha20Poly1305::new(key.as_ref().into())
            .encrypt_in_place_detached([0u8; 12][..].into(), &[], &mut ciphertext[..input_len])
            .unwrap();

        ciphertext[input_len..].copy_from_slice(&tag);

        assert_eq!(input_len + AEAD_TAG_SIZE, ciphertext.len());

        Ok(EncryptedNote { ciphertext, ephem_public })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct EncryptedNote {
    ciphertext: Vec<u8>,
    ephem_public: PublicKey,
}

impl EncryptedNote {
    pub fn decrypt(&self, secret: &SecretKey) -> Result<Note> {
        let shared_secret = sapling_ka_agree(secret, &self.ephem_public);
        let key = kdf_sapling(&shared_secret, &self.ephem_public);

        let output_len = self.ciphertext.len() - AEAD_TAG_SIZE;

        let mut plaintext = vec![0_u8; output_len];
        plaintext.copy_from_slice(&self.ciphertext[..output_len]);

        match ChaCha20Poly1305::new(key.as_ref().into()).decrypt_in_place_detached(
            [0u8; 12][..].into(),
            &[],
            &mut plaintext,
            self.ciphertext[output_len..].into(),
        ) {
            Ok(()) => Ok(Note::decode(&plaintext[..])?),
            Err(e) => Err(Error::NoteDecryptionFailed(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::keypair::Keypair;
    use pasta_curves::group::ff::Field;

    #[test]
    fn test_note_encdec() {
        let note = Note {
            serial: DrkSerial::random(&mut OsRng),
            value: 110,
            token_id: DrkTokenId::random(&mut OsRng),
            coin_blind: DrkCoinBlind::random(&mut OsRng),
            value_blind: DrkValueBlind::random(&mut OsRng),
            token_blind: DrkValueBlind::random(&mut OsRng),
            memo: vec![32, 223, 231, 3, 1, 1],
        };

        let keypair = Keypair::random(&mut OsRng);

        let encrypted_note = note.encrypt(&keypair.public).unwrap();
        let note2 = encrypted_note.decrypt(&keypair.secret).unwrap();
        assert_eq!(note.value, note2.value);
        assert_eq!(note.token_id, note2.token_id);
        assert_eq!(note.token_blind, note2.token_blind);
        assert_eq!(note.memo, note2.memo);
    }
}
