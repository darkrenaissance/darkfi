/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 * Copyright (C) 2021 MONOLOG (Taeho Francis Lim and Jongwhan Lee) MIT License
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

use std::{cmp::Ordering, ops::Range};

use super::{
    utils::{bit, bytes_to_int, len_lcp, offsets},
    BitsLen,
};
use crate::GenericResult;

/// `BitVec` implementation based on bytes slice.
#[derive(Debug, Clone)]
pub struct Bits<'a> {
    pub path: &'a [u8],
    pub range: Range<BitsLen>,
}

impl<'a> Bits<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { path: bytes, range: 0..(bytes.len() as BitsLen * 8) }
    }

    /// Construct `Bits` instance by deserializing bytes slice.
    pub fn from_bytes(bytes: &'a [u8]) -> Self {
        let u = std::mem::size_of::<BitsLen>();
        let start: BitsLen = bytes_to_int(&bytes[..u]);
        let end: BitsLen = bytes_to_int(&bytes[u..2 * u]);
        Self { path: &bytes[2 * u..], range: start..end }
    }

    /// Serialize `Bits` into bytes.
    pub fn to_bytes(&self) -> GenericResult<Vec<u8>> {
        Ok([&self.range.start.to_be_bytes(), &self.range.end.to_be_bytes(), self.path].concat())
    }

    /// Get the very first bit.
    pub fn first(&self) -> bool {
        bit(self.path, self.range.start)
    }

    pub fn len(&self) -> BitsLen {
        self.range.end - self.range.start
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0 || self.path.len() == 0
    }

    /// Get the resulting `Bits` when shifted with the given size.
    pub fn shift(&self, n: BitsLen, tail: bool) -> Self {
        let (q, range) = offsets(&self.range, n, tail);
        if tail {
            Self { path: &self.path[..q as usize], range }
        } else {
            Self { path: &self.path[q as usize..], range }
        }
    }

    /// Get length of the longest common prefix bits for the given two `Bits`.
    pub fn len_common_bits(a: &Self, b: &Self) -> BitsLen {
        len_lcp(a.path, &a.range, b.path, &b.range)
    }

    /// Get the bit at position `i` within this Bits range
    pub fn bit(&self, i: BitsLen) -> bool {
        assert!(i < self.len(), "Bit index out of range");
        bit(self.path, self.range.start + i)
    }

    /// Compare bits lexicographically (MSB to LSB)
    pub fn lexical_cmp(&self, other: &Self) -> Ordering {
        let min_len = std::cmp::min(self.len(), other.len());

        // Compare bit by bit from start of range
        for i in 0..min_len {
            match (self.bit(i), other.bit(i)) {
                (false, true) => return Ordering::Less,
                (true, false) => return Ordering::Greater,
                _ => continue,
            }
        }

        // All compared bits equal, compare lengths
        self.len().cmp(&other.len())
    }
}

// Implement equality/ordering based on actual bit values
impl PartialEq for Bits<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && (0..self.len()).all(|i| self.bit(i) == other.bit(i))
    }
}

impl Eq for Bits<'_> {}

#[allow(clippy::non_canonical_partial_ord_impl)]
impl PartialOrd for Bits<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.lexical_cmp(other))
    }
}

impl Ord for Bits<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.lexical_cmp(other)
    }
}
