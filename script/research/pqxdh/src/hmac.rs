/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! HMAC simplementation.
//! https://en.wikipedia.org/wiki/Hmac
use digest::{
    core_api::Block, crypto_common::BlockSizeUser, Digest, FixedOutput, Output, OutputSizeUser,
    Update,
};

const IPAD: u8 = 0x36;
const OPAD: u8 = 0x5C;

fn get_der_key<D: Digest + BlockSizeUser + Clone>(key: &[u8]) -> Block<D> {
    let mut der_key = Block::<D>::default();
    // The key that HMAC processes must be the same as the block size
    // of the underlying hash function. If the provided key is smaller
    // than that, we just pad it with zeroes. If it's larger, we hash
    // it and then pad it with zeroes.
    if key.len() <= der_key.len() {
        der_key[..key.len()].copy_from_slice(key);
        return der_key
    }

    let hash = D::digest(key);
    // All commonly used hash functions have block size bigger than
    // output hash size, but to be extra rigorous we handle the
    // potential uncommon cases as well. The condition is calculated
    // at compile time, so this branch gets removed from final binary.
    if hash.len() <= der_key.len() {
        der_key[..hash.len()].copy_from_slice(&hash);
    } else {
        let n = der_key.len();
        der_key.copy_from_slice(&hash[..n]);
    }

    der_key
}

/// HMAC for arbitrary hash functions that implement `Digest`
/// and `BlockSizeUser` traits.
#[derive(Clone)]
pub struct Hmac<D: Digest + BlockSizeUser + Clone> {
    digest: D,
    opad_key: Block<D>,
}

impl<D: Digest + BlockSizeUser + Clone> Hmac<D> {
    /// Initialize a new `Hmac` with the given key.
    #[inline]
    pub fn new_from_slice(key: &[u8]) -> Self {
        let der_key = get_der_key::<D>(key);

        let mut ipad_key = der_key.clone();
        for b in ipad_key.iter_mut() {
            *b ^= IPAD;
        }

        let mut digest = D::new();
        digest.update(&ipad_key);

        let mut opad_key = der_key;
        for b in opad_key.iter_mut() {
            *b ^= OPAD;
        }

        Self { digest, opad_key }
    }

    /// Finalize the HMAC
    pub fn finalize(self) -> Output<D> {
        Output::<D>::clone_from_slice(&self.finalize_fixed())
    }
}

impl<D: Digest + BlockSizeUser + Clone> FixedOutput for Hmac<D> {
    fn finalize_into(self, out: &mut Output<Self>) {
        let mut h = D::new();
        h.update(&self.opad_key);
        h.update(&self.digest.finalize());
        h.finalize_into(out);
    }
}

impl<D: Digest + BlockSizeUser + Clone> OutputSizeUser for Hmac<D> {
    type OutputSize = D::OutputSize;
}

impl<D: Digest + BlockSizeUser + Clone> Update for Hmac<D> {
    /// Update the HMAC with the given data.
    fn update(&mut self, data: &[u8]) {
        self.digest.update(data);
    }
}
