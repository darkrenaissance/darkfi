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

/// Maximum allowed k param (circuit rows = 2^k)
pub(super) const MAX_K: u32 = 16;

/// Maximum allowed namespace length in bytes
pub(super) const MAX_NS_LEN: usize = 32;

/// Minimum size allowed for a syntactically valid ZkBinary
/// MAGIC_BYTES.length = 4;
/// `k = ##;` = 6 (because the current upper-limit for k is a two-digit number
/// Therefore 4 + 6 = 10 is the minimum size
pub(super) const MIN_BIN_SIZE: usize = 10;

/// Maximum allowed binary size (1M)
pub(super) const MAX_BIN_SIZE: usize = 1024 * 1024;

/// Maximum number of constants allowed
pub(super) const MAX_CONSTANTS: usize = 1024;

/// Maximum number of literals allowed
pub(super) const MAX_LITERALS: usize = 4096;

/// Maximum number of witnesses allowed
pub(super) const MAX_WITNESSES: usize = 4096;

/// Maximum number of opcodes allowed
pub(super) const MAX_OPCODES: usize = 4096;

/// Maximum number of arguments per opcode
pub(super) const MAX_ARGS_PER_OPCODE: usize = 256;

/// Maximum total heap size (constants + witnesses + assigned variables)
pub(super) const MAX_HEAP_SIZE: usize = MAX_CONSTANTS + MAX_WITNESSES + MAX_OPCODES;

/// Maximum string length for names
pub(super) const MAX_STRING_LEN: usize = 1024;

/// Allowed fields for proofs
pub(super) const ALLOWED_FIELDS: [&str; 1] = ["pallas"];

/// Maximum recursion depth for nested function calls
pub(super) const MAX_RECURSION_DEPTH: usize = 16;

// Section markers in the binary format
pub(super) const SECTION_CONSTANT: &[u8] = b".constant";
pub(super) const SECTION_LITERAL: &[u8] = b".literal";
pub(super) const SECTION_WITNESS: &[u8] = b".witness";
pub(super) const SECTION_CIRCUIT: &[u8] = b".circuit";
pub(super) const SECTION_DEBUG: &[u8] = b".debug";
