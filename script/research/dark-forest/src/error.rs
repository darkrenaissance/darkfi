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

/// Main result type used by this library.
pub type DarkTreeResult<T> = std::result::Result<T, DarkTreeError>;

/// General library errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum DarkTreeError {
    #[error("Invalid DarkLeaf index found: {0} (Expected: {1}")]
    InvalidLeafIndex(usize, usize),

    #[error("Invalid DarkLeaf parent index found for leaf: {0}")]
    InvalidLeafParentIndex(usize),

    #[error("Invalid DarkLeaf children index found for leaf: {0}")]
    InvalidLeafChildrenIndexes(usize),
}
