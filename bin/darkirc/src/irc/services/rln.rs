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

use darkfi_sdk::{bridgetree, crypto::SecretKey};
use darkfi_serial::{async_trait, SerialDecodable, SerialEncodable};

/// Rate-Limit Nullifier account data
#[derive(Debug, Copy, Clone, SerialEncodable, SerialDecodable)]
pub struct RlnIdentity {
    /// Identity nullifier secret
    pub identity_nullifier: SecretKey,
    /// Identity trapdoor secret
    pub identity_trapdoor: SecretKey,
    /// Leaf position of the identity commitment in the accounts' Merkle tree
    pub leaf_pos: bridgetree::Position,
}
