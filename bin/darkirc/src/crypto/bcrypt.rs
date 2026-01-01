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

use bcrypt::DEFAULT_COST;

/// Salt used for the IRC server connection password
pub const BCRYPT_PASSWORD_SALT: [u8; 16] = [
    0x22, 0x23, 0xff, 0x41, 0x57, 0x47, 0x48, 0xfe, 0xde, 0xca, 0x1c, 0xd1, 0x94, 0xef, 0xcc, 0xaa,
];

/// Encrypt the given password with bcrypt-2b
pub fn bcrypt_hash_password<P: AsRef<[u8]>>(password: P) -> String {
    bcrypt::hash_with_salt(password, DEFAULT_COST, BCRYPT_PASSWORD_SALT)
        .unwrap()
        .format_for_version(bcrypt::Version::TwoB)
}
