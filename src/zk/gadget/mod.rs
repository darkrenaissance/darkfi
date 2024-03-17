/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

/// Base field arithmetic gadget
pub mod arithmetic;

/// Small range check, 0..8 bits
pub mod small_range_check;

/// Field-native range check gadget with a lookup table
pub mod native_range_check;

/// Field-native less than comparison gadget with a lookup table
pub mod less_than;

/// is_zero comparison gadget
pub mod is_zero;

/// is_equal comparison gadget
pub mod is_equal;

/// Conditional selection
pub mod cond_select;

/// Conditional selection based on lhs (will output lhs if lhs==0, otherwise rhs)
pub mod zero_cond;

/// Poseidon-based sparse Merkle tree chip
pub mod smt;
pub mod smt2;
