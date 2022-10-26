//! https://signal.org/docs/specifications/x3dh/x3dh.pdf
use std::collections::{HashMap, VecDeque};

use aes_gcm_siv::{AeadInPlace, Aes256GcmSiv, KeyInit};
use anyhow::Result;
use rand::rngs::OsRng;
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519SecretKey};

mod hkdf;
use hkdf::Hkdf;

mod hmac;

mod xeddsa;
use xeddsa::{XeddsaSigner, XeddsaVerifier};

/// The server contains published identity keys and prekeys.
#[derive(Default)]
struct Server(HashMap<X25519PublicKey, Keyset>);

impl Server {
    pub fn upload(&mut self, ik: X25519PublicKey, keyset: Keyset) {
        self.0.insert(ik, keyset);
    }

    pub fn fetch(&mut self, ik: &X25519PublicKey) -> Option<Bundle> {
        if let Some(keyset) = self.0.get_mut(ik) {
            // The server should provide one one-time prekey if one exists,
            // and then delete it. If all of the one-time prekeys have been
            // deleted, the bundle will not contain a one-time prekey.
            let onetime_prekey = keyset.onetime_prekeys.pop_front();

            return Some(Bundle {
                identity_key: *ik,
                signed_prekey: keyset.signed_prekey,
                prekey_signature: keyset.prekey_signature,
                onetime_prekey,
            })
        }

        None
    }
}

/// The set of elliptic curve public keys sent uploaded to a server
struct Keyset {
    pub signed_prekey: X25519PublicKey,
    pub prekey_signature: [u8; 64],
    pub onetime_prekeys: VecDeque<X25519PublicKey>,
}

/// The bundle is a structure returned by the server when requesting
/// it for a certain identity key
struct Bundle {
    pub identity_key: X25519PublicKey,
    pub signed_prekey: X25519PublicKey,
    pub prekey_signature: [u8; 64],
    pub onetime_prekey: Option<X25519PublicKey>,
}

/// Initial message sent from Alice to Bob (see below how it's used)
struct InitialMessage {
    pub identity_key: X25519PublicKey,
    pub ephemeral_key: X25519PublicKey,
    pub prekey_used: Option<X25519PublicKey>,
    pub ciphertext: Vec<u8>,
}

fn main() -> Result<()> {
    // The "server" contains published identity keys and prekeys.
    let mut server = Server::default();

    // The X3DH protocol has three phases:
    // 1. Bob publishes his identity key and prekeys to a server.
    // 2. Alice fetches a "prekey bundle" from the server, and uses
    //    it to send an initial message to Bob.
    // 3. Bob receives and processes Alice's initial message.

    // Alice's identity key `IK_A`
    let alice_ik_secret = X25519SecretKey::new(&mut OsRng);
    let alice_ik_public = X25519PublicKey::from(&alice_ik_secret);

    // Bob's identity key `IK_B`
    let bob_ik_secret = X25519SecretKey::new(&mut OsRng);
    let bob_ik_public = X25519PublicKey::from(&bob_ik_secret);

    // Bob only needs to upload his identity key to the server once.
    // However, Bob may upload new one-time prekeys at other times
    // (e.g. when the server informs Bob that the server's store
    // of one-time prekeys is getting low).
    // Bob will also upload a new signed prekey and prekey signature
    // at some interval (e.g. once a week/month). The new signed prekey
    // and prekey signature will replace the previous values.

    // Bob's signed prekey `SPK_B`
    let bob_spk_secret = X25519SecretKey::new(&mut OsRng);
    let bob_spk_public = X25519PublicKey::from(&bob_spk_secret);

    // Bob's prekey signature `Sig(IK_b, Encode(SPK_B))`
    let nonce = [0_u8; 64];
    let bob_spk_sig = bob_ik_secret.xeddsa_sign(&bob_spk_public.to_bytes(), &nonce);

    // A set of Bob's one-time prekeys `(OPK_B1, OPK_B2, OPK_B3, ...)`
    let mut bob_opk_secrets = vec![
        X25519SecretKey::new(&mut OsRng),
        X25519SecretKey::new(&mut OsRng),
        X25519SecretKey::new(&mut OsRng),
    ];
    let mut bob_opk_publics = VecDeque::new();
    bob_opk_publics.push_back(X25519PublicKey::from(&bob_opk_secrets[0]));
    bob_opk_publics.push_back(X25519PublicKey::from(&bob_opk_secrets[1]));
    bob_opk_publics.push_back(X25519PublicKey::from(&bob_opk_secrets[2]));

    let bob_keyset = Keyset {
        signed_prekey: bob_spk_public,
        prekey_signature: bob_spk_sig,
        onetime_prekeys: bob_opk_publics.clone(),
    };

    // Bob uploads his keyset to the server.
    server.upload(bob_ik_public, bob_keyset);

    // To perform an X3DH key agreement with Bob, Alice contacts the server
    // and fetches a "prekey bundle" containing the following values:
    // * Bob's identity key `IK_B`
    // * Bob's signed prekey `SPK_B`
    // * Bob's prekey signature `Sig(IK_B, Encode(SPK_B))`
    // * (Optionally) Bob's one-time prekey `OPK_B`
    let bob_keyset = server.fetch(&bob_ik_public).unwrap();

    // Alice verifies the prekey signature and aborts the protocol if
    // verification fails.
    assert!(bob_keyset
        .identity_key
        .xeddsa_verify(&bob_keyset.signed_prekey.to_bytes(), &bob_keyset.prekey_signature));

    // Alice then generates an ephemeral keypair with public key `EK_A`
    let alice_ek_secret = X25519SecretKey::new(&mut OsRng);
    let alice_ek_public = X25519PublicKey::from(&alice_ek_secret);

    // If the bundle does _not_ contain a one-time prekey, she calculates:
    // DH1 = DH(IK_A, SPK_B)
    // DH2 = DH(EK_A, IK_B)
    // DH3 = DH(EK_A, SPK_B)
    // SK = KDF(DH1 || DH2 || DH3)
    // If the bundle _does_ contain a one-time prekey, additionally she
    // does another dh:
    // DH4 = DH(EK_A, OPK_B)
    // SK = KDF(DH1 || DH2 || DH3 || DH4)
    let dh1 = alice_ik_secret.diffie_hellman(&bob_keyset.signed_prekey);
    let dh2 = alice_ek_secret.diffie_hellman(&bob_keyset.identity_key);
    let dh3 = alice_ek_secret.diffie_hellman(&bob_keyset.signed_prekey);
    let mut dh4 = None;
    if let Some(opk) = bob_keyset.onetime_prekey {
        dh4 = Some(alice_ek_secret.diffie_hellman(&opk));
    }

    // KDF represents 32 bytes of output from the HKDF algorithm with inputs:
    // - HKDF input key material = F || KM, where KM is an input byte sequence
    //   containing secret key material, and F is a byte sequence containing
    //   32 0xFF bytes when the curve is X25519. F is used for cryptographic
    //   domain separation with XEdDSA.
    // - HKDF salt = A zero-filled byte sequence equal to the hash output length.
    // - HKDF info - The info parameter.
    let info = b"x3dh_info";
    let salt = [0u8; 32];
    let mut ikm = vec![0xFF; 32];
    ikm.extend_from_slice(&dh1.to_bytes());
    ikm.extend_from_slice(&dh2.to_bytes());
    ikm.extend_from_slice(&dh3.to_bytes());
    if let Some(ref opk_dh) = dh4 {
        ikm.extend_from_slice(&opk_dh.to_bytes());
    }

    let hkdf = Hkdf::<Sha256>::new(&salt, &ikm);
    let mut sk = [0u8; 32];
    hkdf.expand(info.as_ref(), &mut sk).unwrap();

    // After calculating SK, Alice deletes her ephemeral private key and the
    // DH outputs.
    drop(alice_ek_secret);
    drop(dh1);
    drop(dh2);
    drop(dh3);
    drop(dh4);

    // Alice then calculates an "associated data" byte sequence AD that
    // contains identity information for both parties:
    // AD = Encode(IK_A) || Encode(IK_B)
    // Alice may optionally append additional info to AD, such as Alice
    // and Bob's usernames, certificates, or other identifying information.
    let mut ad = Vec::with_capacity(64);
    ad.extend_from_slice(&alice_ik_public.to_bytes());
    ad.extend_from_slice(&bob_ik_public.to_bytes());

    // Alice then sends Bob an initial message containing:
    // - Alice's identity key IK_A
    // - Alice's ephemeral key EK_A
    // - Identifiers stating which of Bob's prekeys Alice used
    // - An initial ciphertext with some AEAD encryption scheme using AD as
    //   associated data and using an encryption key which is either SK
    //   or the output of some cryptographic PRF keyed by SK.
    const AEAD_TAG_SIZE: usize = 16;

    let message = b"ohai bob";
    let mut ciphertext = vec![0u8; message.len() + AEAD_TAG_SIZE];
    ciphertext[..message.len()].copy_from_slice(message);

    let nonce = [0u8; 12][..].into();
    Aes256GcmSiv::new(&sk.into()).encrypt_in_place(nonce, &ad, &mut ciphertext).unwrap();

    let initial_message = InitialMessage {
        identity_key: alice_ik_public,
        ephemeral_key: alice_ek_public,
        prekey_used: bob_keyset.onetime_prekey,
        ciphertext,
    };

    // Upon receiving Alice's initial message, Bob retrieves Alice's
    // identity key and ephemeral key from the message. Bob also loads
    // his identity private key, and the private key(s) corresponding
    // to whichever signed prekey and one-time prekey (if any) Alice used.
    // NOTE: In this example, we assume Bob already knows the latest prekey
    //       he signed and uploaded to the server.

    // Using these keys, Bob repeats the DH and KDF calculations from the
    // previous section to derive SK, and then deletes the DH values.
    let mut onetime_prekey = None;
    if let Some(opk_used) = initial_message.prekey_used {
        for i in bob_opk_secrets.clone() {
            if X25519PublicKey::from(&i.clone()) == opk_used {
                onetime_prekey = Some(i);
            }
        }
    }

    let dh1 = bob_spk_secret.diffie_hellman(&initial_message.identity_key);
    let dh2 = bob_ik_secret.diffie_hellman(&initial_message.ephemeral_key);
    let dh3 = bob_spk_secret.diffie_hellman(&initial_message.ephemeral_key);
    let mut dh4 = None;
    if let Some(ref opk) = onetime_prekey {
        dh4 = Some(opk.diffie_hellman(&initial_message.ephemeral_key));
    }

    let info = b"x3dh_info";
    let salt = [0u8; 32];
    let mut ikm = vec![0xFF; 32];
    ikm.extend_from_slice(&dh1.to_bytes());
    ikm.extend_from_slice(&dh2.to_bytes());
    ikm.extend_from_slice(&dh3.to_bytes());
    if let Some(ref opk_dh) = dh4 {
        ikm.extend_from_slice(&opk_dh.to_bytes());
    }

    let hkdf = Hkdf::<Sha256>::new(&salt, &ikm);
    let mut sk2 = [0u8; 32];
    hkdf.expand(info.as_ref(), &mut sk2).unwrap();
    assert_eq!(sk, sk2); // Just to confirm everything's correct

    // Bob then constructs the AD byte sequence using IK_A and IK_B
    // as Alice did above.
    let mut ad = Vec::with_capacity(64);
    ad.extend_from_slice(&initial_message.identity_key.to_bytes());
    ad.extend_from_slice(&bob_ik_public.to_bytes());

    // Finally, Bob attempts to decrypt the initial ciphertext using SK and AD.
    // If the initial ciphertext fails to decrypt, Bob aborts the protocol and
    // deletes SK.
    let mut plaintext = vec![0_u8; initial_message.ciphertext.len()];
    plaintext.copy_from_slice(&initial_message.ciphertext);

    let nonce = [0u8; 12][..].into();
    Aes256GcmSiv::new(&sk2.into()).decrypt_in_place(nonce, &ad, &mut plaintext).unwrap();
    plaintext.resize(plaintext.len() - AEAD_TAG_SIZE, 0);

    assert_eq!(plaintext, message); // Just to confirm everything's correct

    // If the initial ciphertext decrypts successfully, the protocol is complete
    // for Bob. Bob deletes any one-time prekey secret key that was used, for
    // forward secrecy. Bob may then continue using SK or keys derived from SK
    // within the post-X3DH protocol for communication with Alice.
    if let Some(opk) = onetime_prekey {
        bob_opk_secrets.retain(|x| x.to_bytes() != opk.to_bytes());
    }

    Ok(())
}
