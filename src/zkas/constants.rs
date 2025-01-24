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

/// Maximum allowed k param (circuit rows = 2^k)
pub const MAX_K: u32 = 16;

/// Maximum allowed namespace length in bytes
pub const MAX_NS_LEN: usize = 32;

/// Minimum size allowed for a syntactically valid ZkBinary
/// MAGIC_BYTES.length = 4;
/// `k = ##;` = 6 (because the current upper-limit for k is a two-digit number
/// Therefore 4 + 6 = 10 is the minimum size
pub const MIN_BIN_SIZE: usize = 10;

/// Allowed fields for proofs
pub const ALLOWED_FIELDS: [&str; 1] = ["pallas"];
