/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

//! Taken from https://docs.rs/ockam_vault/latest/src/ockam_vault/xeddsa.rs.html
//! XEdDSA according to <https://signal.org/docs/specifications/xeddsa/#xeddsa>
use curve25519_dalek::{
    constants::ED25519_BASEPOINT_TABLE,
    montgomery::MontgomeryPoint,
    scalar::{clamp_integer, Scalar},
};
use digest::Digest;
use ed25519_dalek::{Signature, VerifyingKey as Ed25519PublicKey};
use sha2::Sha512;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519SecretKey};

pub trait XeddsaSigner {
    fn xeddsa_sign(&self, msg: &[u8], nonce: &[u8; 64]) -> [u8; 64];
}

pub trait XeddsaVerifier {
    fn xeddsa_verify(&self, msg: &[u8], nonce: &[u8; 64]) -> bool;
}

impl XeddsaSigner for X25519SecretKey {
    fn xeddsa_sign(&self, msg: &[u8], nonce: &[u8; 64]) -> [u8; 64] {
        //
        // PREPARATION OF THE KEY MATERIAL
        //
        // This algorithm to sign data using a Curve25519 keypair has to
        // tackle two issues. The first issue is that the conversion of
        // a Curve25519 public key to an Ed25519 public key is not unique
        // when only having access to the u coordinate of the Curve25519
        // public key, which is the case with the serialization format
        // commonly used. In fact the conversion is unique by the sign of
        // the Ed25519 public key x coordinate. This signing algorithm
        // "solves" the problem by modifying the private key so that the
        // sign of the resulting Ed25519 public key is always zero.

        // x25519-dalek private keys are already clamped, so just compute
        // the Ed25519 public key from the Curve25519 private key.
        let scalar_k = Scalar::from_bytes_mod_order(clamp_integer(self.to_bytes()));
        let edward_point = ED25519_BASEPOINT_TABLE * &scalar_k;
        let mut compressed_edwards = edward_point.compress();
        let sign = compressed_edwards.0[31] >> 7;
        // Set the sign bit to zero after adjusting the private key
        compressed_edwards.0[31] &= 0x7F; // A.s = 0

        // Compute the negative secret key

        // If the sign bit of the calculated Ed25519 public key is zero,
        // the private key doesn't have to be touched. If the sign bit
        // is one, the private key has to be inverted prior to using it.
        let k = if sign == 1 { -scalar_k } else { scalar_k };

        //
        // SIGNING
        //
        // The second problem this algorithm has to tackle is that
        // Ed25519 signature algorithms don't use the private scalar
        // directly, but rather use a seed to derive other data from.
        // To create signatures compatible with Ed25519, a modified
        // version of the signing algorithm is required that does not
        // depend on a seed.
        // r = hash1(a || M || Z) (mod q)
        let mut hash_padding = [0xff, 32];
        hash_padding[0] = 0xfe;
        let mut hasher = Sha512::new();
        hasher.update(hash_padding);
        hasher.update(k.as_bytes());
        hasher.update(msg);
        hasher.update(nonce.as_ref());
        let r = Scalar::from_hash(hasher);

        // R = rB
        let cap_r = (ED25519_BASEPOINT_TABLE * &r).compress();

        // h = hash(R || A || M) (mod q)
        hasher = Sha512::new();
        hasher.update(cap_r.as_bytes());
        hasher.update(compressed_edwards.as_bytes());
        hasher.update(msg);
        let h = Scalar::from_hash(hasher);

        // s = r + ha (mod q)
        let s = r + h * k;

        // return R || s
        let mut sig = [0u8; 64];
        sig[..32].copy_from_slice(cap_r.as_bytes());
        sig[32..].copy_from_slice(s.as_bytes());
        sig
    }
}

impl XeddsaVerifier for X25519PublicKey {
    fn xeddsa_verify(&self, msg: &[u8], sig: &[u8; 64]) -> bool {
        let pt = MontgomeryPoint(self.to_bytes());

        if let Some(edwards) = pt.to_edwards(0) {
            let pk = Ed25519PublicKey::from_bytes(&edwards.compress().to_bytes()).unwrap();
            let signature = Signature::from_bytes(sig);
            return pk.verify_strict(msg, &signature).is_ok()
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn xeddsa_test() {
        let nonce = [0u8; 64];
        let msg = [0u8; 200];

        let xsecret_key = X25519SecretKey::new(&mut OsRng);
        let xpublic_key = X25519PublicKey::from(&xsecret_key);

        let sig = xsecret_key.xeddsa_sign(&msg, &nonce);
        assert!(xpublic_key.xeddsa_verify(&msg, &sig));
    }
}
