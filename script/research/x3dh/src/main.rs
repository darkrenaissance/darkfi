/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

//! https://signal.org/docs/specifications/x3dh/x3dh.pdf
//! https://signal.org/docs/specifications/doubleratchet/doubleratchet.pdf
use std::collections::{HashMap, VecDeque};

use aes_gcm_siv::{AeadInPlace, Aes256GcmSiv, KeyInit};
use digest::Update;
use rand::rngs::OsRng;
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519SecretKey};

mod hkdf;
use hkdf::Hkdf;

mod hmac;
use hmac::Hmac;

mod xeddsa;
use xeddsa::{XeddsaSigner, XeddsaVerifier};

const AEAD_TAG_SIZE: usize = 16;

const MESSAGE_KEY_CONSTANT: u8 = 0x01;
const CHAIN_KEY_CONSTANT: u8 = 0x02;

const X3DH_INIT_INFO: &[u8] = b"x3dh_double_ratchet_init";

const BLANK_NONCE: &[u8] = &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

// wat do?
const MAX_SKIP: u64 = 500;

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

#[derive(Copy, Clone, Debug)]
struct MessageHeader {
    /// Ratchet public key
    dh: X25519PublicKey,
    /// Previous chain length
    pn: u64,
    /// Message number
    n: u64,
}

impl MessageHeader {
    /// Creates a new message header containing the DH ratchet public key
    /// `dh` the previous chain length `pn`, and the message number `n`.
    pub fn new(dh: &X25519SecretKey, pn: u64, n: u64) -> Self {
        Self { dh: X25519PublicKey::from(dh), pn, n }
    }

    pub fn to_bytes(self) -> [u8; 48] {
        let mut ret = [0u8; 48];
        ret[..32].copy_from_slice(&self.dh.to_bytes());
        ret[32..40].copy_from_slice(&self.pn.to_le_bytes());
        ret[40..].copy_from_slice(&self.n.to_le_bytes());
        ret
    }

    pub fn from_bytes(arr: [u8; 48]) -> Self {
        let pk_bytes: [u8; 32] = arr[..32].try_into().unwrap();
        let dh = X25519PublicKey::from(pk_bytes);
        let pn = u64::from_le_bytes(arr[32..40].try_into().unwrap());
        let n = u64::from_le_bytes(arr[40..].try_into().unwrap());
        Self { dh, pn, n }
    }

    /// Returns the AEAD encryption of the message header with header key `hk`.
    /// Because the same `hk` will be used repeatedly, the AEAD nonce must
    /// either be a stateful non-repeating value, or must be a random
    /// non-repeating value chosen with at least 128 bits of entropy.
    pub fn encrypt(&self, hk: [u8; 32], ad: &[u8]) -> Vec<u8> {
        // FIXME: BUG: Don't reuse the nonce.
        let nonce = [0u8; 12][..].into();

        let mut ciphertext = vec![0u8; 48 + AEAD_TAG_SIZE];
        ciphertext[..48].copy_from_slice(&self.to_bytes());

        Aes256GcmSiv::new(&hk.into()).encrypt_in_place(nonce, ad, &mut ciphertext).unwrap();
        ciphertext
    }

    /// Returns the authenticated decryption of `ciphertext` with header key `hk`.
    pub fn decrypt(ciphertext: &[u8], hk: [u8; 32], ad: &[u8]) -> Option<Self> {
        // FIXME: BUG: Don't reuse the nonce.
        let nonce = [0u8; 12][..].into();

        let mut plaintext = vec![0u8; ciphertext.len()];
        plaintext.copy_from_slice(ciphertext);

        match Aes256GcmSiv::new(&hk.into()).decrypt_in_place(nonce, ad, &mut plaintext) {
            Ok(()) => {
                plaintext.resize(plaintext.len() - AEAD_TAG_SIZE, 0);
                let message_header = Self::from_bytes(plaintext.try_into().unwrap());
                Some(message_header)
            }
            Err(_) => None,
        }
    }
}

/// Returns a pair (32-byte chain key, 32-byte message key) as the output of
/// applying a KDF keyed by a 32-byte chain key `ck` to some constant.
/// HMAC with SHA256 is recommended, using `ck` as the HMAC key and using
/// separate constants as input (e.g. a single byte 0x01 as input to produce
/// the message key, and a single byte 0x02 as input to produce the next chain
/// key.
fn kdf_ck(ck: [u8; 32]) -> ([u8; 32], [u8; 32]) {
    let mut hmac = Hmac::<Sha256>::new_from_slice(&ck);
    hmac.update(&[CHAIN_KEY_CONSTANT]);
    let chain_key = hmac.finalize();

    let mut hmac = Hmac::<Sha256>::new_from_slice(&ck);
    hmac.update(&[MESSAGE_KEY_CONSTANT]);
    let message_key = hmac.finalize();

    (chain_key.into(), message_key.into())
}

/// Returns a new root key, chain key, and next header key as the output
/// of applying a KDF keyed by root key `rk` to a Diffie-Hellman output
/// `dh_out`.
/// This function is recommended to be implemented using HKDF with SHA256
/// using `rk` as HKDF salt, `dh_out` as HKDF input key material, and an
/// application-specific byte sequence as HKDF info. The info value should
/// be chosen to be distinct from other uses of HKDF in the application.
fn kdf_rk(rk: [u8; 32], dh_out: [u8; 32]) -> ([u8; 32], [u8; 32], [u8; 32]) {
    const KDF_RK_INFO: &[u8] = b"x3dh_double_ratchet_kdf_rk";
    const KDF_HE_INFO: &[u8] = b"x3dh_double_ratchet_kdf_rk_he";

    let (_root_key, hkdf) = Hkdf::<Sha256>::extract(&rk, &dh_out);
    let mut chain_key = [0u8; 32];
    hkdf.expand(KDF_RK_INFO, &mut chain_key).unwrap();

    let (root_key, hkdf) = Hkdf::<Sha256>::extract(&rk, &dh_out);
    let mut next_header_key = [0u8; 32];
    hkdf.expand(KDF_HE_INFO, &mut next_header_key).unwrap();

    (root_key.into(), chain_key, next_header_key)
}

#[derive(Clone)]
struct DoubleRatchetSessionState {
    /// DH ratchet key pair (the "sending" or "self" ratchet key) (DHRs)
    pub dh_sending: X25519SecretKey,
    /// DH ratchet public key (the "received" or "remote" key) (DHRr)
    pub dh_remote: X25519PublicKey,
    /// 32-byte root key (RK)
    pub root_key: [u8; 32],
    /// 32-byte Chain Key for sending (CKs)
    pub chain_key_send: [u8; 32],
    /// 32-byte Chain Key for receiving (CKr)
    pub chain_key_recv: [u8; 32],
    /// Message numbers for sending (Ns)
    pub n_send: u64,
    /// Message numbers for receiving (Nr)
    pub n_recv: u64,
    /// Number of messages in previous sending chain (PN)
    pub n_prev: u64,
    /// Dictionary of skipped-over message keys, indexed by header key
    /// and message number. Raises an exception if too many elements
    /// are stored.
    pub mkskipped: HashMap<([u8; 32], u64), [u8; 32]>,
    /// 32-byte Header Key for sending (HKs)
    pub header_key_send: [u8; 32],
    /// 32-byte Header Key for receiving (HKr)
    pub header_key_recv: [u8; 32],
    /// 32-byte Next Header Key for sending (NHKs)
    pub next_header_key_send: [u8; 32],
    /// 32-byte Next Header Key for receiving (NHKr)
    pub next_header_key_recv: [u8; 32],
}

impl DoubleRatchetSessionState {
    /// This function performs a symmetric-key ratchet step, then encrypts
    /// the message with the resulting message key. In addition to the
    /// message's _plaintext_ it takes an AD byte sequence which is
    /// prepended to the header to form the associated data for the
    // underlying AEAD encryption.
    pub fn ratchet_encrypt(&mut self, plaintext: &[u8], ad: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let (chain_key, message_key) = kdf_ck(self.chain_key_send);
        self.chain_key_send = chain_key;
        println!("ENCRYPT(): new chain send: {:?}", &chain_key[..5]);

        let header = MessageHeader::new(&self.dh_sending, self.n_prev, self.n_send);
        let enc_header = header.encrypt(self.header_key_send, &[]);

        let mut associated_data = Vec::with_capacity(ad.len() + enc_header.len());
        associated_data.extend_from_slice(ad);
        associated_data.extend_from_slice(&enc_header);

        let mut ciphertext = vec![0u8; plaintext.len() + AEAD_TAG_SIZE];
        ciphertext[..plaintext.len()].copy_from_slice(plaintext);

        // Because each message key is only used once, the AEAD nonce may be
        // handled in several ways:
        // * Fixed to a constant
        // * Derived from `mk` alongside an independent AEAD encryption key
        // * Derived as an additional output from HMAC
        // * Chosen randomly and transmitted

        // ENCRYPT(message_key, plaintext, (AD || enc_header))
        println!("ENCRYPT(): message key: {:?}", &message_key[..5]);
        Aes256GcmSiv::new(&message_key.into())
            .encrypt_in_place(BLANK_NONCE.into(), &associated_data, &mut ciphertext)
            .unwrap();

        self.n_send += 1;

        (enc_header, ciphertext)
    }

    /// Decrypt messages. This function does the following:
    /// * If the message corresponds to a skipped message key this function
    ///   decrypts the message, deletes the message key, and returns.
    /// * Otherwise, if a new ratchet key has been received, this function
    ///   stores any skipped message keys from the receiving chain and
    ///   performs a DH ratchet step to replace the sending and receiving
    ///   chains.
    /// * This function then stores any skipped message keys from the current
    ///   receiving chain, performs a symmetric-key ratchet step to derive
    ///   the relevant message key and next chain key, and decrypts the msg.
    /// If an exception is raised (e.g. message authentication failure), then
    /// the message is discarded and changes to the state object are discarded.
    /// Otherwise, the decrypted plaintext is accepted and changes to the state
    /// object are stored.
    pub fn ratchet_decrypt(&mut self, enc_header: &[u8], ciphertext: &[u8], ad: &[u8]) -> Vec<u8> {
        // We clone here so we don't have to worry about mutating the state before
        // everything is correct.
        let mut state = self.clone();

        if let Some(plaintext) = state.try_skipped_message_keys(enc_header, ciphertext, ad) {
            println!("found skipped");
            *self = state;
            return plaintext
        }

        if let Some((header, dh_ratchet)) = state.decrypt_header(enc_header) {
            if dh_ratchet {
                state.skip_message_keys(header.pn);
                state.dh_ratchet(header);
            }
            state.skip_message_keys(header.n);
        } else {
            panic!("couldn't decrypt header")
        }

        let (chain_key, message_key) = kdf_ck(state.chain_key_recv);
        state.chain_key_recv = chain_key;
        println!("DECRYPT(): new chain recv: {:?}", &chain_key[..5]);
        state.n_recv += 1;

        let mut plaintext = vec![0u8; ciphertext.len()];
        plaintext.copy_from_slice(ciphertext);

        let mut associated_data = Vec::with_capacity(ad.len() + enc_header.len());
        associated_data.extend_from_slice(ad);
        associated_data.extend_from_slice(enc_header);

        // DECRYPT(message_key, ciphertext, (AD || enc_header))
        println!("DECRYPT(): message key: {:?}", &message_key[..5]);
        Aes256GcmSiv::new(&message_key.into())
            .decrypt_in_place(BLANK_NONCE.into(), &associated_data, &mut plaintext)
            .unwrap();

        // Apply the state change
        *self = state;

        plaintext.resize(plaintext.len() - AEAD_TAG_SIZE, 0);
        plaintext
    }

    fn try_skipped_message_keys(
        &mut self,
        enc_header: &[u8],
        ciphertext: &[u8],
        ad: &[u8],
    ) -> Option<Vec<u8>> {
        let mut plaintext = ciphertext.to_vec();
        let mut rem = None;

        for ((hk, n), mk) in self.mkskipped.iter_mut() {
            if let Some(header) = MessageHeader::decrypt(enc_header, *hk, &[]) {
                if header.n == *n {
                    rem = Some((*hk, *n));
                    let mut associated_data = Vec::with_capacity(ad.len() + enc_header.len());
                    associated_data.extend_from_slice(ad);
                    associated_data.extend_from_slice(enc_header);

                    let mk = *mk;
                    Aes256GcmSiv::new(&mk.into())
                        .decrypt_in_place(BLANK_NONCE.into(), &associated_data, &mut plaintext)
                        .unwrap();

                    plaintext.resize(plaintext.len() - AEAD_TAG_SIZE, 0);
                    break
                }
                panic!("Failed to decrypt message from skipped message keys");
            }
        }

        if let Some(found) = rem {
            self.mkskipped.remove(&found);
            return Some(plaintext)
        }

        None
    }

    fn decrypt_header(&mut self, enc_header: &[u8]) -> Option<(MessageHeader, bool)> {
        if let Some(header) = MessageHeader::decrypt(enc_header, self.header_key_recv, &[]) {
            return Some((header, false))
        }

        if let Some(header) = MessageHeader::decrypt(enc_header, self.next_header_key_recv, &[]) {
            return Some((header, true))
        }

        println!("Failed to decrypt header");
        None
    }

    fn skip_message_keys(&mut self, until: u64) {
        if self.n_recv + MAX_SKIP < until {
            panic!("I can't hold all of these lemons");
        }

        if self.chain_key_recv != [0u8; 32] {
            while self.n_recv < until {
                let (chain_key_recv, message_key) = kdf_ck(self.chain_key_recv);
                self.chain_key_recv = chain_key_recv;
                println!("SKIP(): new chain recv: {:?}", &chain_key_recv[..5]);
                self.mkskipped.insert((self.header_key_recv, self.n_recv), message_key);
                self.n_recv += 1;
            }
        }
    }

    fn dh_ratchet(&mut self, header: MessageHeader) {
        self.n_prev = self.n_send;
        self.n_send = 0;
        self.n_recv = 0;
        self.header_key_send = self.next_header_key_send;
        self.header_key_recv = self.next_header_key_recv;
        self.dh_remote = header.dh;

        let hkdf_ikm = self.dh_sending.diffie_hellman(&self.dh_remote);
        (self.root_key, self.chain_key_recv, self.next_header_key_recv) =
            kdf_rk(self.root_key, hkdf_ikm.to_bytes());

        let dh_secret_new = X25519SecretKey::new(OsRng);
        self.dh_sending = dh_secret_new;

        let hkdf_ikm = self.dh_sending.diffie_hellman(&self.dh_remote);
        (self.root_key, self.chain_key_send, self.next_header_key_send) =
            kdf_rk(self.root_key, hkdf_ikm.to_bytes());
    }
}

fn main() {
    // The "server" contains published identity keys and prekeys.
    let mut server = Server::default();

    // The X3DH protocol has three phases:
    // 1. Bob publishes his identity key and prekeys to a server.
    // 2. Alice fetches a "prekey bundle" from the server, and uses
    //    it to send an initial message to Bob.
    // 3. Bob receives and processes Alice's initial message.

    // Alice's identity key `IK_A`
    let alice_ik_secret = X25519SecretKey::new(OsRng);
    let alice_ik_public = X25519PublicKey::from(&alice_ik_secret);

    // Bob's identity key `IK_B`
    let bob_ik_secret = X25519SecretKey::new(OsRng);
    let bob_ik_public = X25519PublicKey::from(&bob_ik_secret);

    // Bob only needs to upload his identity key to the server once.
    // However, Bob may upload new one-time prekeys at other times
    // (e.g. when the server informs Bob that the server's store
    // of one-time prekeys is getting low).
    // Bob will also upload a new signed prekey and prekey signature
    // at some interval (e.g. once a week/month). The new signed prekey
    // and prekey signature will replace the previous values.

    // Bob's signed prekey `SPK_B`
    let bob_spk_secret = X25519SecretKey::new(OsRng);
    let bob_public_spk = X25519PublicKey::from(&bob_spk_secret);

    // Bob's prekey signature `Sig(IK_b, Encode(SPK_B))`
    let nonce = [0_u8; 64];
    let bob_spk_signature = bob_ik_secret.xeddsa_sign(&bob_public_spk.to_bytes(), &nonce);

    // A set of Bob's one-time prekeys `(OPK_B1, OPK_B2, OPK_B3, ...)`
    let mut bob_opk_secrets =
        vec![X25519SecretKey::new(OsRng), X25519SecretKey::new(OsRng), X25519SecretKey::new(OsRng)];
    let mut bob_opk_publics = VecDeque::new();
    bob_opk_publics.push_back(X25519PublicKey::from(&bob_opk_secrets[0]));
    bob_opk_publics.push_back(X25519PublicKey::from(&bob_opk_secrets[1]));
    bob_opk_publics.push_back(X25519PublicKey::from(&bob_opk_secrets[2]));

    let bob_keyset = Keyset {
        signed_prekey: bob_public_spk,
        prekey_signature: bob_spk_signature,
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
    let alice_ek_secret = X25519SecretKey::new(OsRng);
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
    hkdf.expand(X3DH_INIT_INFO, &mut sk).unwrap();

    // After calculating SK, Alice deletes her ephemeral private key and the
    // DH outputs.
    // TODO: Actually erase
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
    let message = b"ohai bob";
    let mut ciphertext = vec![0u8; message.len() + AEAD_TAG_SIZE];
    ciphertext[..message.len()].copy_from_slice(message);

    Aes256GcmSiv::new(&sk.into())
        .encrypt_in_place(BLANK_NONCE.into(), &ad, &mut ciphertext)
        .unwrap();

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

    let salt = [0u8; 32];
    let mut ikm = vec![0xFF; 32];
    ikm.extend_from_slice(&dh1.to_bytes());
    ikm.extend_from_slice(&dh2.to_bytes());
    ikm.extend_from_slice(&dh3.to_bytes());
    if let Some(ref opk_dh) = dh4 {
        ikm.extend_from_slice(&opk_dh.to_bytes());
    }

    // TODO: Erase ephemeral data

    let hkdf = Hkdf::<Sha256>::new(&salt, &ikm);
    let mut sk2 = [0u8; 32];
    hkdf.expand(X3DH_INIT_INFO, &mut sk2).unwrap();
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

    Aes256GcmSiv::new(&sk2.into())
        .decrypt_in_place(BLANK_NONCE.into(), &ad, &mut plaintext)
        .unwrap();
    plaintext.resize(plaintext.len() - AEAD_TAG_SIZE, 0);

    assert_eq!(plaintext, message); // Just to confirm everything's correct

    // If the initial ciphertext decrypts successfully, the protocol is complete
    // for Bob. Bob deletes any one-time prekey secret key that was used, for
    // forward secrecy. Bob may then continue using SK or keys derived from SK
    // within the post-X3DH protocol for communication with Alice.
    if let Some(opk) = onetime_prekey {
        bob_opk_secrets.retain(|x| x.to_bytes() != opk.to_bytes());
    }

    // =======================+
    // Double Ratchet with X3DH
    // ========================

    // * The SK output from X3DH becomes the SK input to Double Ratchet initialization.
    // * The AD output from X3DH becomes the AD input to Double Ratchet {en,de}cryption.
    // * Bob's signed prekey SPK_B becomes Bob's initial ratchet public key (and
    //   corresponding keypair) for Double Ratchet initialization.

    // Any Double Ratchet message encrypted using Alice's initial sending chain can
    // serve as an "initial ciphertext" for X3DH. To deal with the possibility of
    // lost or out-of-order messages, a recommended pattern is for Alice to repeatedly
    // send the same X3DH initial message prepended to all of her Double Ratchet
    // messages until she receives Bob's first Double Ratchet response message.

    // Once Alice and Bob have agreed on SK and Bob's ratchet public key, Alice
    // and Bob initialize their states:

    // Alice:
    let alice_dh_secret = X25519SecretKey::new(OsRng);

    // The X3DH secret becomes the HKDF salt, and the ikm is the DH output
    // of Alice's DH secret and Bob's SPK_B.
    let hkdf_ikm = alice_dh_secret.diffie_hellman(&bob_keyset.signed_prekey);
    let (root_key, chain_key_send, next_header_key_send) = kdf_rk(sk, hkdf_ikm.to_bytes());

    // TODO: We're using SK here as the initial header encryption keys. Perhaps it's not safe?
    let mut ars = DoubleRatchetSessionState {
        dh_sending: alice_dh_secret,
        dh_remote: bob_keyset.signed_prekey,
        root_key,
        chain_key_send,
        chain_key_recv: [0u8; 32],
        n_send: 0,
        n_recv: 0,
        n_prev: 0,
        mkskipped: HashMap::default(),
        header_key_send: sk,
        header_key_recv: [0u8; 32],
        next_header_key_send,
        next_header_key_recv: sk,
    };

    // Bob:
    let mut brs = DoubleRatchetSessionState {
        dh_sending: bob_spk_secret,
        dh_remote: X25519PublicKey::from([0u8; 32]),
        root_key: sk,
        chain_key_send: [0u8; 32],
        chain_key_recv: [0u8; 32],
        n_send: 0,
        n_recv: 0,
        n_prev: 0,
        mkskipped: HashMap::default(),
        header_key_send: [0u8; 32],
        header_key_recv: [0u8; 32],
        next_header_key_send: sk,
        next_header_key_recv: sk,
    };

    // TODO: What kind of AD should be used?
    // Alice sends it to Bob, and Bob decrypts.
    let message_to_bob = b"hai bobz";
    println!("Alice: n_recv={}, n_send={}, n_prev={}", ars.n_recv, ars.n_send, ars.n_prev);
    let (enc_header, ciphertext) = ars.ratchet_encrypt(message_to_bob, &[]);
    println!("Bob: n_recv={}, n_send={}, n_prev={}", brs.n_recv, brs.n_send, brs.n_prev);
    let plaintext = brs.ratchet_decrypt(&enc_header, &ciphertext, &[]);
    assert_eq!(plaintext, message_to_bob);
    println!("Bob decrypted message: {}", String::from_utf8_lossy(&plaintext));

    // Bob replies to Alice.
    let message_to_alice = b"hai alice, what's up?";
    println!("Bob: n_recv={}, n_send={}, n_prev={}", brs.n_recv, brs.n_send, brs.n_prev);
    let (enc_header, ciphertext) = brs.ratchet_encrypt(message_to_alice, &[]);
    println!("Alice: n_recv={}, n_send={}, n_prev={}", ars.n_recv, ars.n_send, ars.n_prev);
    let plaintext = ars.ratchet_decrypt(&enc_header, &ciphertext, &[]);
    assert_eq!(plaintext, message_to_alice);
    println!("Alice decrypted message: {}", String::from_utf8_lossy(&plaintext));

    // Alice loves Bob.
    let message_to_bob = b"you schizo";
    println!("Alice: n_recv={}, n_send={}, n_prev={}", ars.n_recv, ars.n_send, ars.n_prev);
    let (enc_header, ciphertext) = ars.ratchet_encrypt(message_to_bob, &[]);
    println!("Bob: n_recv={}, n_send={}, n_prev={}", brs.n_recv, brs.n_send, brs.n_prev);
    let plaintext = brs.ratchet_decrypt(&enc_header, &ciphertext, &[]);
    assert_eq!(plaintext, message_to_bob);
    println!("Bob decrypted message: {}", String::from_utf8_lossy(&plaintext));

    // Let's try out of order
    let message_to_bob1 = b"hello";
    let message_to_bob2 = b"jello";
    let (enc_header1, ciphertext1) = ars.ratchet_encrypt(message_to_bob1, &[]);
    let (enc_header2, ciphertext2) = ars.ratchet_encrypt(message_to_bob2, &[]);

    // Slow Bob
    let plaintext = brs.ratchet_decrypt(&enc_header2, &ciphertext2, &[]);
    assert_eq!(plaintext, message_to_bob2);
    let plaintext = brs.ratchet_decrypt(&enc_header1, &ciphertext1, &[]);
    assert_eq!(plaintext, message_to_bob1);

    let message_to_alice = b"weaponised autism";
    let (enc_header, ciphertext) = brs.ratchet_encrypt(message_to_alice, &[]);
    let plaintext = ars.ratchet_decrypt(&enc_header, &ciphertext, &[]);
    assert_eq!(plaintext, message_to_alice);
}
