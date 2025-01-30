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

//! HMAC-based Extract-and-Expand Key Derivation Function (HKDF)
//! https://tools.ietf.org/html/rfc5869
use core::fmt;

use digest::{
    crypto_common::BlockSizeUser, typenum::Unsigned, Digest, Output, OutputSizeUser, Update,
};

use super::hmac::Hmac;

/// Structure for InvalidPrkLength, used for output error handling.
#[derive(Copy, Clone, Debug)]
pub struct InvalidPrkLength;

impl fmt::Display for InvalidPrkLength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        f.write_str("invalid pseudorandom key length, too short")
    }
}

/// Structure for InvalidLength, used for output error handling.
#[derive(Copy, Clone, Debug)]
pub struct InvalidLength;

impl fmt::Display for InvalidLength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        f.write_str("invalid number of blocks, too large output")
    }
}

/// HKDF-Extract for arbitrary hash functions implementing `Digest`
/// and `BlockSizeUser` traits.
#[derive(Clone)]
pub struct HkdfExtract<H: Digest + BlockSizeUser + Clone> {
    hmac: Hmac<H>,
}

impl<H: Digest + BlockSizeUser + Clone> HkdfExtract<H> {
    /// Iniitialize a new `HkdfExtract` with the given salt.
    pub fn new(salt: &[u8]) -> Self {
        Self { hmac: Hmac::<H>::new_from_slice(salt) }
    }

    /// Feeds in additional input key material to the HKDF-Extract context.
    pub fn input_ikm(&mut self, ikm: &[u8]) {
        self.hmac.update(ikm);
    }

    /// Completes the HKDF-Extract operation, returning both the generated
    /// pseudorandom key and `Hkdf` struct for expanding.
    pub fn finalize(self) -> (Output<H>, Hkdf<H>) {
        let prk = self.hmac.finalize();
        let hkdf = Hkdf::from_prk(&prk).expect("PRK size is correct");
        (prk, hkdf)
    }
}

/// Structure representing the HKDF, capable of HKDF-Expand
/// and HKDF-Extract operations.
#[derive(Clone)]
pub struct Hkdf<H: Digest + BlockSizeUser + Clone> {
    hmac: Hmac<H>,
}

impl<H: Digest + BlockSizeUser + Clone> Hkdf<H> {
    /// Convenience method for `extract` when the generated pseudorandom
    /// key can be ignored and only the HKDF-Expand operation is needed.
    pub fn new(salt: &[u8], ikm: &[u8]) -> Self {
        let (_, hkdf) = Self::extract(salt, ikm);
        hkdf
    }

    /// HKDF-Extract operation returning both the generated pseudorandom
    /// key and `Hkdf` struct for expanding.
    pub fn extract(salt: &[u8], ikm: &[u8]) -> (Output<H>, Self) {
        let mut extract_ctx = HkdfExtract::new(salt);
        extract_ctx.input_ikm(ikm);
        extract_ctx.finalize()
    }

    /// Create `Hkdf` from an already cryptographically strong pseudorandom key.
    pub fn from_prk(prk: &[u8]) -> Result<Self, InvalidPrkLength> {
        if prk.len() < <H as OutputSizeUser>::OutputSize::to_usize() {
            return Err(InvalidPrkLength)
        }

        Ok(Self { hmac: Hmac::<H>::new_from_slice(prk) })
    }

    /// HKDF-Expand operation. If you don't have any `info` to pass, use
    /// an empty slice.
    pub fn expand(&self, info: &[u8], okm: &mut [u8]) -> Result<(), InvalidLength> {
        self.expand_multi_info(&[info], okm)
    }

    pub fn expand_multi_info(&self, infos: &[&[u8]], okm: &mut [u8]) -> Result<(), InvalidLength> {
        let mut prev: Option<Output<H>> = None;

        let chunk_len = <H as OutputSizeUser>::OutputSize::USIZE;
        if okm.len() > chunk_len * 255 {
            return Err(InvalidLength)
        }

        for (block_n, block) in okm.chunks_mut(chunk_len).enumerate() {
            let mut hmac = self.hmac.clone();

            if let Some(ref prev) = prev {
                hmac.update(prev);
            }

            // Feed in the info components in sequence. This is equivalent
            // to feeding in the concatenation of all the info components.
            for info in infos {
                hmac.update(info);
            }

            hmac.update(&[block_n as u8 + 1]);

            let output = hmac.finalize();
            let block_len = block.len();
            block.copy_from_slice(&output[..block_len]);

            prev = Some(output);
        }

        Ok(())
    }
}
