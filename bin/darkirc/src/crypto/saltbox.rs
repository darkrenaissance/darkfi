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

use crypto_box::{
    aead::{Aead, AeadCore},
    ChaChaBox,
};
use rand::rngs::OsRng;

/// Encrypt given data using the given `ChaChaBox`.
/// Returns base58-encoded string of the ciphertext.
/// Panics if encryption fails.
///
/// The encryption format we're using with `ChaChaBox` is `nonce||ciphertext`,
/// where nonce is 24 bytes large, and the remaining data should be the ciphertext.
pub fn encrypt(salt_box: &ChaChaBox, plaintext: &[u8]) -> String {
    // Generate the nonce
    let nonce = ChaChaBox::generate_nonce(&mut OsRng);

    // Encrypt
    let mut ciphertext = salt_box.encrypt(&nonce, plaintext).unwrap();

    // Concatenate
    let mut concat = Vec::with_capacity(24 + ciphertext.len());
    concat.append(&mut nonce.as_slice().to_vec());
    concat.append(&mut ciphertext);

    // Encode
    bs58::encode(concat).into_string()
}

/// Attempt to decrypt given ciphertext using the given `ChaChaBox`.
/// Returns a `Vec<u8>` on success, and `None` on failure.
///
/// The encryption format we're using with `ChaChaBox` is `nonce||ciphertext`,
/// where nonce is 24 bytes large, and the remaining data should be the ciphertext.
pub fn try_decrypt(salt_box: &ChaChaBox, ciphertext: &[u8]) -> Option<Vec<u8>> {
    // Make sure we have enough bytes to work with
    if ciphertext.len() < 25 {
        return None
    }

    salt_box.decrypt((&ciphertext[0..24]).into(), &ciphertext[24..]).ok()
}
