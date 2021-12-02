use std::io;

use crypto_api_chachapoly::ChachaPolyIetf;
use rand::rngs::OsRng;

use crate::{
    crypto::{
        diffie_hellman::{kdf_sapling, sapling_ka_agree},
        keypair::{PublicKey, SecretKey},
    },
    serial::{Decodable, Encodable, ReadExt, WriteExt},
    types::*,
    Error, Result,
};

pub const NOTE_PLAINTEXT_SIZE: usize = 32 +    // serial
    8 +     // value
    32 +    // token_id
    32 +    // coin_blind
    32; // value_blind
pub const AEAD_TAG_SIZE: usize = 16;
pub const ENC_CIPHERTEXT_SIZE: usize = NOTE_PLAINTEXT_SIZE + AEAD_TAG_SIZE;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Note {
    pub serial: DrkSerial,
    pub value: u64,
    pub token_id: DrkTokenId,
    pub coin_blind: DrkCoinBlind,
    pub value_blind: DrkValueBlind,
}

impl Encodable for Note {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.serial.encode(&mut s)?;
        len += self.value.encode(&mut s)?;
        len += self.token_id.encode(&mut s)?;
        len += self.coin_blind.encode(&mut s)?;
        len += self.value_blind.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for Note {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            serial: Decodable::decode(&mut d)?,
            value: Decodable::decode(&mut d)?,
            token_id: Decodable::decode(&mut d)?,
            coin_blind: Decodable::decode(&mut d)?,
            value_blind: Decodable::decode(d)?,
        })
    }
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

pub struct EncryptedNote {
    ciphertext: [u8; ENC_CIPHERTEXT_SIZE],
    ephem_public: PublicKey,
}

impl Encodable for EncryptedNote {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        s.write_slice(&self.ciphertext)?;
        len += ENC_CIPHERTEXT_SIZE;
        len += self.ephem_public.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for EncryptedNote {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let mut ciphertext = [0u8; ENC_CIPHERTEXT_SIZE];
        d.read_slice(&mut ciphertext[..])?;
        Ok(Self { ciphertext, ephem_public: Decodable::decode(d)? })
    }
}

impl EncryptedNote {
    pub fn decrypt(&self, secret: &SecretKey) -> Result<Note> {
        let shared_secret = sapling_ka_agree(&secret, &self.ephem_public);
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

#[test]
fn test_note_encdec() {
    use crate::types::*;

    let note = Note {
        serial: DrkSerial::random(&mut OsRng),
        value: 110,
        token_id: DrkTokenId::random(&mut OsRng),
        coin_blind: DrkCoinBlind::random(&mut OsRng),
        value_blind: DrkValueBlind::random(&mut OsRng),
    };

    let secret = DrkSecretKey::random(&mut OsRng);
    let public = derive_public_key(secret);

    let encrypted_note = note.encrypt(&public).unwrap();
    let note2 = encrypted_note.decrypt(&secret).unwrap();
    assert_eq!(note.value, note2.value);
    assert_eq!(note.token_id, note2.token_id);
}
