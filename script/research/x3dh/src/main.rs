//! https://signal.org/docs/specifications/x3dh/x3dh.pdf
use anyhow::Result;
use crypto_api_chachapoly::ChachaPolyIetf;
use rand::rngs::OsRng;
use sha2::Sha256;
use x25519_dalek::{
    EphemeralSecret, PublicKey as X25519PublicKey, StaticSecret as X25519SecretKey,
};

mod hkdf;
use hkdf::Hkdf;
mod hmac;
mod xeddsa;
use xeddsa::{XeddsaSigner, XeddsaVerifier};

// 3.2 Publishing keys
// Bob only needs to upload his identity key to the server once.
// However, Bob may upload new one-time prekeys at other times.
// Bob will also upload a new signed prekey and prekey signature
// at some interval (e.g. once a week/month).
// The new signed prekey and prekey signature will replace old values.
struct Keyset {
    pub identity_key: X25519PublicKey,
    pub signed_prekey: X25519PublicKey,
    pub prekey_signature: [u8; 64],
    //pub onetime_prekeys: Vec<X25519PublicKey>,
}

struct InitialMessage {
    pub identity_key: X25519PublicKey,
    pub ephemeral_key: X25519PublicKey,
    pub prekeys_used: Vec<X25519PublicKey>,
    pub ciphertext: Vec<u8>,
}

fn main() -> Result<()> {
    let mut server: Vec<Keyset> = vec![];

    // Alice's identity key
    let alice_ik_secret = X25519SecretKey::new(&mut OsRng);
    let alice_ik_public = X25519PublicKey::from(&alice_ik_secret);

    // Bob's identity key
    let bob_ik_secret = X25519SecretKey::new(&mut OsRng);
    let bob_ik_public = X25519PublicKey::from(&bob_ik_secret);

    // Bob's signed prekey
    let bob_spk_secret = X25519SecretKey::new(&mut OsRng);
    let bob_spk_public = X25519PublicKey::from(&bob_spk_secret);

    // Bob's prekey signature
    let nonce = [0_u8; 64];
    let bob_spk_signature = bob_ik_secret.xeddsa_sign(&bob_spk_public.to_bytes(), &nonce);

    // Bob uploads his keyset to the server
    // TODO: onetime_prekeys
    let keyset = Keyset {
        identity_key: bob_ik_public,
        signed_prekey: bob_spk_public,
        prekey_signature: bob_spk_signature,
        //onetime_prekeys: vec![],
    };
    server.push(keyset);

    // Alice contacts the server and fetches a "prekey bundle" of Bob's keys:
    // NOTE: Only one onetime_prekey should be in the bundle.
    let bundle = &server[0];

    // Alice verifies the prekey signature and aborts if verification fails:
    // NOTE: Should Alice have Bob's key from somewhere else?
    // NOTE: Or should there be an additional key that links to the keyset?
    assert!(bundle
        .identity_key
        .xeddsa_verify(&bundle.signed_prekey.to_bytes(), &bundle.prekey_signature));

    // Then Alice creates an ephemeral key pair with the public key EK_A
    let ek_a_secret = X25519SecretKey::new(&mut OsRng);
    let ek_a_public = X25519PublicKey::from(&ek_a_secret);

    // If the bundle does not contain a one-time prekey, Alice calculates:
    // DH1 = DH(IK_A, SPK_B)
    // DH2 = DH(EK_A, IK_B)
    // DH3 = DH(EK_A, SPK_B)
    // SK = KDF(DH1 || DH2 || DH3)
    // If the bundle _does_ contain a one-time prekey, an additional DH is
    // calculated:
    // DH4 = DH(EK_A, OPK_B)
    // SK = KDF(DH1 || DH2 || DH3 || DH4)
    let dh1 = alice_ik_secret.diffie_hellman(&bundle.signed_prekey);
    let dh2 = ek_a_secret.diffie_hellman(&bundle.identity_key);
    let dh3 = ek_a_secret.diffie_hellman(&bundle.signed_prekey);

    let mut ikm = vec![0xFF; 32];
    ikm.extend_from_slice(&dh1.to_bytes());
    ikm.extend_from_slice(&dh2.to_bytes());
    ikm.extend_from_slice(&dh3.to_bytes());

    let info = b"x3dh_info";
    let salt = [0_u8; 32];
    let hkdf = Hkdf::<Sha256>::new(&salt, &ikm);
    let mut sk = [0u8; 32];
    hkdf.expand(&info.to_vec(), &mut sk).unwrap();

    // Alice then calculates an "associated data" byte sequence AD
    // that contains:
    // AD = Encode(IK_A) || Encode(IK_B)
    // Alice may optionally append additional information to AD
    let mut ad = Vec::with_capacity(64);
    ad.extend_from_slice(&alice_ik_public.to_bytes());
    ad.extend_from_slice(&bob_ik_public.to_bytes());

    let first_msg = b"hi";
    const AEAD_TAG_SIZE: usize = 16;
    let mut ciphertext = vec![0_u8; first_msg.len() + AEAD_TAG_SIZE];
    assert_eq!(
        ChachaPolyIetf::aead_cipher()
            .seal_to(&mut ciphertext, first_msg, &ad, &sk, &[0u8; 12])
            .unwrap(),
        first_msg.len() + AEAD_TAG_SIZE
    );

    // Alice then sends Bob an initial message:
    let initial_msg = InitialMessage {
        identity_key: alice_ik_public,
        ephemeral_key: ek_a_public,
        prekeys_used: vec![],
        ciphertext,
    };

    // Bob receives the initial message and repeats the DH and KDF
    let dh1 = bob_spk_secret.diffie_hellman(&initial_msg.identity_key);
    let dh2 = bob_ik_secret.diffie_hellman(&initial_msg.ephemeral_key);
    let dh3 = bob_spk_secret.diffie_hellman(&initial_msg.ephemeral_key);

    let mut ikm = vec![0xFF; 32];
    ikm.extend_from_slice(&dh1.to_bytes());
    ikm.extend_from_slice(&dh2.to_bytes());
    ikm.extend_from_slice(&dh3.to_bytes());

    let info = b"x3dh_info";
    let salt = [0_u8; 32];
    let hkdf = Hkdf::<Sha256>::new(&salt, &ikm);
    let mut sk2 = [0u8; 32];
    hkdf.expand(&info.to_vec(), &mut sk2).unwrap();
    assert_eq!(sk, sk2);

    let mut plaintext = vec![0; initial_msg.ciphertext.len() - AEAD_TAG_SIZE];
    ChachaPolyIetf::aead_cipher()
        .open_to(&mut plaintext, &initial_msg.ciphertext, &ad, &sk2, &[0u8; 12])
        .unwrap();

    assert_eq!(plaintext, first_msg);

    Ok(())
}
