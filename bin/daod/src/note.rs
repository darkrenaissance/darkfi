use crypto_api_chachapoly::ChachaPolyIetf;
use rand::rngs::OsRng;

use darkfi::{
    crypto::{
        diffie_hellman::{kdf_sapling, sapling_ka_agree},
        keypair::{PublicKey, SecretKey},
    },
    util::serial::{Decodable, Encodable, SerialDecodable, SerialEncodable},
    Error, Result,
};

pub const AEAD_TAG_SIZE: usize = 16;

pub fn encrypt<T: Encodable>(note: &T, public: &PublicKey) -> Result<EncryptedNote2> {
    let ephem_secret = SecretKey::random(&mut OsRng);
    let ephem_public = PublicKey::from_secret(ephem_secret);
    let shared_secret = sapling_ka_agree(&ephem_secret, public);
    let key = kdf_sapling(&shared_secret, &ephem_public);

    let mut input = Vec::new();
    note.encode(&mut input)?;

    let mut ciphertext = vec![0; input.len() + AEAD_TAG_SIZE];
    assert_eq!(
        ChachaPolyIetf::aead_cipher()
            .seal_to(&mut ciphertext, &input, &[], key.as_ref(), &[0u8; 12])
            .unwrap(),
        input.len() + AEAD_TAG_SIZE
    );

    Ok(EncryptedNote2 { ciphertext, ephem_public })
}

#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct EncryptedNote2 {
    ciphertext: Vec<u8>,
    ephem_public: PublicKey,
}

impl EncryptedNote2 {
    pub fn decrypt<T: Decodable>(&self, secret: &SecretKey) -> Result<T> {
        let shared_secret = sapling_ka_agree(secret, &self.ephem_public);
        let key = kdf_sapling(&shared_secret, &self.ephem_public);

        let mut plaintext = vec![0; self.ciphertext.len()];
        assert_eq!(
            ChachaPolyIetf::aead_cipher()
                .open_to(&mut plaintext, &self.ciphertext, &[], key.as_ref(), &[0u8; 12])
                .map_err(|_| Error::NoteDecryptionFailed)?,
            self.ciphertext.len() - AEAD_TAG_SIZE
        );

        T::decode(&plaintext[..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkfi::crypto::{
        keypair::Keypair,
        types::{DrkCoinBlind, DrkSerial, DrkTokenId, DrkValueBlind},
    };
    use group::ff::Field;

    #[test]
    fn test_note_encdec() {
        #[derive(SerialEncodable, SerialDecodable)]
        struct MyNote {
            serial: DrkSerial,
            value: u64,
            token_id: DrkTokenId,
            coin_blind: DrkCoinBlind,
            value_blind: DrkValueBlind,
            token_blind: DrkValueBlind,
            memo: Vec<u8>,
        }
        let note = MyNote {
            serial: DrkSerial::random(&mut OsRng),
            value: 110,
            token_id: DrkTokenId::random(&mut OsRng),
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
