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

//! https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-vrf-04#section-5
#![allow(non_snake_case)]

use darkfi_sdk::{
    crypto::{
        constants::NullifierK,
        pasta_prelude::{CurveExt, Field, Group},
        util::mod_r_p,
    },
    pasta::{
        group::{ff::FromUniformBytes, GroupEncoding},
        pallas,
    },
};
use halo2_gadgets::ecc::chip::FixedPoint;
use lazy_static::lazy_static;
use rand::rngs::OsRng;

lazy_static! {
    /// The `B` generator
    static ref B: pallas::Affine = NullifierK.generator();
}

const VRF_DOMAIN: &str = "ECVRF";

#[derive(Copy, Clone, Debug)]
struct VrfProof {
    gamma: pallas::Point,
    c: blake3::Hash,
    s: pallas::Scalar,
}

fn prove(x: pallas::Base, alpha_string: &[u8]) -> VrfProof {
    let Y = *B * mod_r_p(x);

    let pallas_hasher = pallas::Point::hash_to_curve(VRF_DOMAIN);
    let H = pallas_hasher(&Y.to_bytes()) + pallas_hasher(alpha_string);

    let gamma = H * mod_r_p(x);
    let k = pallas::Scalar::random(&mut OsRng);

    let mut hasher = blake3::Hasher::new();
    hasher.update(&H.to_bytes());
    hasher.update(&gamma.to_bytes());
    hasher.update(&(*B * k).to_bytes());
    hasher.update(&(H * k).to_bytes());
    let c = hasher.finalize();

    let mut c_scalar = [0u8; 64];
    c_scalar[..blake3::OUT_LEN].copy_from_slice(c.as_bytes());
    let c_scalar = pallas::Scalar::from_uniform_bytes(&c_scalar);

    let s = k + c_scalar * mod_r_p(x);

    VrfProof { gamma, c, s }
}

fn verify(Y: pallas::Point, proof: VrfProof, alpha_string: &[u8]) -> bool {
    let pallas_hasher = pallas::Point::hash_to_curve(VRF_DOMAIN);
    let H = pallas_hasher(&Y.to_bytes()) + pallas_hasher(alpha_string);

    let mut c = [0u8; 64];
    c[..blake3::OUT_LEN].copy_from_slice(proof.c.as_bytes());
    let c_scalar = pallas::Scalar::from_uniform_bytes(&c);

    let U = *B * proof.s - Y * c_scalar;
    let V = H * proof.s - proof.gamma * c_scalar;

    let mut hasher = blake3::Hasher::new();
    hasher.update(&H.to_bytes());
    hasher.update(&proof.gamma.to_bytes());
    hasher.update(&U.to_bytes());
    hasher.update(&V.to_bytes());

    hasher.finalize() == proof.c
}

fn main() {
    // VRF secret key
    let secret_key = pallas::Base::random(&mut OsRng);
    // VRF public key
    let public_key = *B * mod_r_p(secret_key);
    // VRF input
    let input = [0xde, 0xad, 0xbe, 0xef];

    let proof = prove(secret_key, &input);
    assert!(verify(public_key, proof, &input));

    // Forged public key
    let forged_public_key = pallas::Point::random(&mut OsRng);
    assert!(!verify(forged_public_key, proof, &input));

    // Forged input
    let forged_input = [0xde, 0xad, 0xba, 0xbe];
    assert!(!verify(public_key, proof, &forged_input));
}
