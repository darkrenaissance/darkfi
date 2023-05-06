/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 * Copyright (C) 2021 Silur <https://github.com/Silur/ECVRF/>
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

use darkfi_sdk::{
    crypto::{
        constants::NullifierK,
        pasta_prelude::{CurveExt, Field},
        util::mod_r_p,
        Keypair, PublicKey, SecretKey,
    },
    pasta::{
        group::{ff::FromUniformBytes, GroupEncoding},
        pallas,
    },
};
use halo2_gadgets::ecc::chip::FixedPoint;
use rand::rngs::OsRng;

struct VrfProof {
    gamma: pallas::Point,
    c: [u8; 32],
    s: pallas::Scalar,
}

fn keygen() -> Keypair {
    Keypair::random(&mut OsRng)
}

/// The output of a VRF function is the VRF hash and the proof to verify
/// we generated this hash with the supplied key.
fn prove(input: &[u8], secret_key: &SecretKey) -> ([u8; 32], VrfProof) {
    let h = pallas::Point::hash_to_curve("ecvrf")(input);
    let gamma = h * mod_r_p(secret_key.inner());
    let k = pallas::Scalar::random(&mut OsRng);

    // TODO: Valid to use this as basepoint?
    let g = NullifierK.generator();

    let mut hasher = blake3::Hasher::new();
    // TODO: Basepoint?
    hasher.update(&g.to_bytes());
    hasher.update(&h.to_bytes());
    hasher.update(&(g * mod_r_p(secret_key.inner())).to_bytes());
    hasher.update(&(h * mod_r_p(secret_key.inner())).to_bytes());
    hasher.update(&(g * k).to_bytes());
    hasher.update(&(h * k).to_bytes());

    let mut c = [0_u8; 64];
    let binding = hasher.finalize();
    let hres = binding.as_bytes();
    for i in 0..hres.len() {
        c[i] = hres[i];
    }

    let c_scalar = pallas::Scalar::from_uniform_bytes(&c);
    let s = k - c_scalar * mod_r_p(secret_key.inner());
    let beta = blake3::hash(&gamma.to_bytes());

    (beta.into(), VrfProof { gamma, c: c[..32].try_into().unwrap(), s })
}

fn verify(input: &[u8], pubkey: &PublicKey, output: &[u8; 32], proof: &VrfProof) -> bool {
    let mut c = [0_u8; 64];
    for i in 0..proof.c.len() {
        c[i] = proof.c[i];
    }

    // TODO: Valid to use this as basepoint?
    let g = NullifierK.generator();

    let c_scalar = pallas::Scalar::from_uniform_bytes(&c);
    let u = pubkey.inner() * c_scalar + g * proof.s;
    let h = pallas::Point::hash_to_curve("ecvrf")(input);
    let v = proof.gamma * c_scalar + h * proof.s;

    let mut hasher = blake3::Hasher::new();
    // TODO: Basepoint?
    hasher.update(&g.to_bytes());
    hasher.update(&h.to_bytes());
    hasher.update(&pubkey.inner().to_bytes());
    hasher.update(&proof.gamma.to_bytes());
    hasher.update(&u.to_bytes());
    hasher.update(&v.to_bytes());

    let mut local_c = [0_u8; 32];
    let binding = hasher.finalize();
    let hres = binding.as_bytes();
    for i in 0..hres.len() {
        local_c[i] = hres[i];
    }

    blake3::hash(&proof.gamma.to_bytes()).as_bytes() == output && local_c == proof.c
}

fn main() {
    let keypair = keygen();
    let input = vec![0xde, 0xad, 0xbe, 0xef];
    let (output, proof) = prove(&input, &keypair.secret);
    assert!(verify(&input, &keypair.public, &output, &proof));

    let input = vec![0xde, 0xad, 0xbe, 0xed];
    assert!(!verify(&input, &keypair.public, &output, &proof));
}
