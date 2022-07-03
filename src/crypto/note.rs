use crypto_api_chachapoly::ChachaPolyIetf;
use rand::rngs::OsRng;

use crate::{
    crypto::{
        diffie_hellman::{kdf_sapling, sapling_ka_agree},
        keypair::{PublicKey, SecretKey},
        types::{DrkCoinBlind, DrkSerial, DrkTokenId, DrkValueBlind},
    },
    util::serial::{Decodable, Encodable, SerialDecodable, SerialEncodable},
    Error, Result,
};

/// Plaintext size is serial + value + token_id + coin_blind + value_blind
pub const NOTE_PLAINTEXT_SIZE: usize = 32 + 8 + 32 + 32 + 32 + 32;
pub const AEAD_TAG_SIZE: usize = 16;
pub const ENC_CIPHERTEXT_SIZE: usize = NOTE_PLAINTEXT_SIZE + AEAD_TAG_SIZE;

#[derive(Copy, Clone, Debug, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Note {
    pub serial: DrkSerial,
    pub value: u64,
    pub token_id: DrkTokenId,
    pub coin_blind: DrkCoinBlind,
    pub value_blind: DrkValueBlind,
    pub token_blind: DrkValueBlind,
}

impl Note {
    pub fn encrypt(&self, public: &PublicKey) -> Result<EncryptedNote> {
        let ephem_secret = SecretKey::random(&mut OsRng);
        let ephem_public = PublicKey::from_secret(ephem_secret);
        let shared_secret = sapling_ka_agree(&ephem_secret, public);
        let key = kdf_sapling(&shared_secret, &ephem_public);

        let mut input = Vec::new();
        self.encode(&mut input)?;

        let mut ciphertext = [0u8; ENC_CIPHERTEXT_SIZE];
        assert_eq!(
            ChachaPolyIetf::aead_cipher()
                .seal_to(&mut ciphertext, &input, &[], key.as_ref(), &[0u8; 12])
                .unwrap(),
            ENC_CIPHERTEXT_SIZE
        );

        Ok(EncryptedNote { ciphertext, ephem_public })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct EncryptedNote {
    ciphertext: [u8; ENC_CIPHERTEXT_SIZE],
    ephem_public: PublicKey,
}

impl EncryptedNote {
    pub fn decrypt(&self, secret: &SecretKey) -> Result<Note> {
        let shared_secret = sapling_ka_agree(secret, &self.ephem_public);
        let key = kdf_sapling(&shared_secret, &self.ephem_public);

        let mut plaintext = [0; ENC_CIPHERTEXT_SIZE];
        assert_eq!(
            ChachaPolyIetf::aead_cipher()
                .open_to(&mut plaintext, &self.ciphertext, &[], key.as_ref(), &[0u8; 12])
                .map_err(|_| Error::NoteDecryptionFailed)?,
            NOTE_PLAINTEXT_SIZE
        );

        Note::decode(&plaintext[..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::keypair::Keypair;
    use group::ff::Field;

    #[test]
    fn test_note_encdec() {
        let note = Note {
            serial: DrkSerial::random(&mut OsRng),
            value: 110,
            token_id: DrkTokenId::random(&mut OsRng),
            coin_blind: DrkCoinBlind::random(&mut OsRng),
            value_blind: DrkValueBlind::random(&mut OsRng),
            token_blind: DrkValueBlind::random(&mut OsRng),
        };

        let keypair = Keypair::random(&mut OsRng);

        let encrypted_note = note.encrypt(&keypair.public).unwrap();
        let note2 = encrypted_note.decrypt(&keypair.secret).unwrap();
        assert_eq!(note.value, note2.value);
        assert_eq!(note.token_id, note2.token_id);
        assert_eq!(note.token_blind, note2.token_blind);
    }
}
