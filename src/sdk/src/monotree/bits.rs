/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::ops::Range;

use super::{
    utils::{bit, bytes_to_int, len_lcp, nbytes_across},
    BitsLen,
};
use crate::GenericResult;

#[derive(Debug, Clone, PartialEq)]
/// `BitVec` implementation based on bytes slice.
pub struct Bits<'a> {
    pub path: &'a [u8],
    pub range: Range<BitsLen>,
}

impl<'a> Bits<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Bits { path: bytes, range: 0..(bytes.len() as BitsLen * 8) }
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
        let start = (self.range.start / 8) as usize;
        let end = self.range.end.div_ceil(8) as usize;
        let mut path = self.path[start..end].to_owned();
        let r = (self.range.start % 8) as u8;
        if r != 0 {
            let mask = 0xffu8 >> r;
            path[0] &= mask;
        }
        let r = (self.range.end % 8) as u8;
        if r != 0 {
            let mask = 0xffu8 << (8 - r);
            let last = path.len() - 1;
            path[last] &= mask;
        }
        Ok([&self.range.start.to_be_bytes(), &self.range.end.to_be_bytes(), &path[..]].concat())
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

    /// Get the first `n` bits.
    pub fn take(&self, n: BitsLen) -> Self {
        let x = self.range.start + n;
        let q = nbytes_across(self.range.start, x);
        let range = self.range.start..x;
        Self { path: &self.path[..q as usize], range }
    }

    /// Skip the first `n` bits.
    pub fn drop(&self, n: BitsLen) -> Self {
        let x = self.range.start + n;
        let q = x / 8;
        let range = x % 8..self.range.end - 8 * (x / 8);
        Self { path: &self.path[q as usize..], range }
    }

    /// Get length of the longest common prefix bits for the given two `Bits`.
    pub fn len_common_bits(a: &Self, b: &Self) -> BitsLen {
        len_lcp(a.path, &a.range, b.path, &b.range)
    }
}
