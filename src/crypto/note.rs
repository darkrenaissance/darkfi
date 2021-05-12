use crypto_api_chachapoly::ChachaPolyIetf;
use ff::Field;
use rand::rngs::OsRng;
use std::io;

use super::diffie_hellman::{kdf_sapling, sapling_ka_agree};
use crate::error::{Error, Result};
use crate::serial::{Decodable, Encodable, ReadExt, WriteExt};

pub const NOTE_PLAINTEXT_SIZE: usize = 32 + // serial
    8 + // value
    32 + // coin_blind
    32; // valcom_blind
pub const AEAD_TAG_SIZE: usize = 16;
pub const ENC_CIPHERTEXT_SIZE: usize = NOTE_PLAINTEXT_SIZE + AEAD_TAG_SIZE;

pub struct Note {
    pub serial: jubjub::Fr,
    pub value: u64,
    pub coin_blind: jubjub::Fr,
    pub valcom_blind: jubjub::Fr,
}

impl Encodable for Note {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.serial.encode(&mut s)?;
        len += self.value.encode(&mut s)?;
        len += self.coin_blind.encode(&mut s)?;
        len += self.valcom_blind.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for Note {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            serial: Decodable::decode(&mut d)?,
            value: Decodable::decode(&mut d)?,
            coin_blind: Decodable::decode(&mut d)?,
            valcom_blind: Decodable::decode(d)?,
        })
    }
}

impl Note {
    pub fn encrypt(&self, public: &jubjub::SubgroupPoint) -> Result<EncryptedNote> {
        let ephem_secret = jubjub::Fr::random(&mut OsRng);
        let ephem_public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * ephem_secret;
        let shared_secret = sapling_ka_agree(&ephem_secret, public.into());
        let key = kdf_sapling(shared_secret, &ephem_public.into());

        let mut input = Vec::new();
        self.encode(&mut input)?;

        let mut ciphertext = [0u8; ENC_CIPHERTEXT_SIZE];
        assert_eq!(
            ChachaPolyIetf::aead_cipher()
                .seal_to(&mut ciphertext, &input, &[], key.as_ref(), &[0u8; 12])
                .unwrap(),
            ENC_CIPHERTEXT_SIZE
        );

        Ok(EncryptedNote {
            ciphertext,
            ephem_public,
        })
    }
}

pub struct EncryptedNote {
    ciphertext: [u8; ENC_CIPHERTEXT_SIZE],
    ephem_public: jubjub::SubgroupPoint,
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
        Ok(Self {
            ciphertext,
            ephem_public: Decodable::decode(d)?,
        })
    }
}

impl EncryptedNote {
    pub fn decrypt(&self, secret: &jubjub::Fr) -> Result<Note> {
        let shared_secret = sapling_ka_agree(&secret, &self.ephem_public.into());
        let key = kdf_sapling(shared_secret, &self.ephem_public.into());

        let mut plaintext = [0; ENC_CIPHERTEXT_SIZE];
        assert_eq!(
            ChachaPolyIetf::aead_cipher()
                .open_to(
                    &mut plaintext,
                    &self.ciphertext,
                    &[],
                    key.as_ref(),
                    &[0u8; 12]
                )
                .map_err(|_| Error::NoteDecryptionFailed)?,
            NOTE_PLAINTEXT_SIZE
        );

        Note::decode(&plaintext[..])
    }
}

#[test]
fn test_note_encdec() {
    let note = Note {
        serial: jubjub::Fr::random(&mut OsRng),
        value: 110,
        coin_blind: jubjub::Fr::random(&mut OsRng),
        valcom_blind: jubjub::Fr::random(&mut OsRng),
    };

    let secret = jubjub::Fr::random(&mut OsRng);
    let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;

    let encrypted_note = note.encrypt(&public).unwrap();
    let note2 = encrypted_note.decrypt(&secret).unwrap();
    assert_eq!(note.value, note2.value);
}
