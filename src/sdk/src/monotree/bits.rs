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

/// An owned representation of a bit path, compatible with Bits serialization.
/// Internally stores bits normalized to start at position 0.
/// Tracks the original offset for correct serialization.
#[derive(Debug, Clone, PartialEq)]
pub struct BitsOwned {
    /// Bits stored normalized (starting at bit 0, MSB-first)
    path: Vec<u8>,
    /// Number of valid bits
    len: BitsLen,
    /// Original starting bit position (for serialization)
    offset: BitsLen,
}

impl BitsOwned {
    /// Create empty `BitsOwned` with given length and offset
    #[inline]
    pub fn new(len: BitsLen, offset: BitsLen) -> Self {
        let num_bytes = len.div_ceil(8) as usize;
        Self { path: vec![0u8; num_bytes], len, offset }
    }

    /// Create from raw normalized bytes (bits start at position 0)
    #[inline]
    fn from_raw(path: Vec<u8>, len: BitsLen, offset: BitsLen) -> Self {
        Self { path, len, offset }
    }

    #[inline]
    pub fn len(&self) -> BitsLen {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn first(&self) -> bool {
        debug_assert!(self.is_empty(), "cannot get first bit of empty BitsOwned");
        self.path[0] & 0x80 != 0
    }

    /// Serialize to bytes in monotree format: `[start:2][end:2][path_bytes]`
    /// Shifts bits to match the original offset position.
    pub fn to_bytes(&self) -> GenericResult<Vec<u8>> {
        let start = self.offset;
        let end = self.offset + self.len;

        let byte_start = (start / 8) as usize;
        let byte_end = end.div_ceil(8) as usize;
        let num_path_bytes = byte_end - byte_start;

        let mut result = Vec::with_capacity(4 + num_path_bytes);
        result.extend_from_slice(&start.to_be_bytes());
        result.extend_from_slice(&end.to_be_bytes());

        if num_path_bytes == 0 {
            return Ok(result)
        }

        let bit_shift = (start % 8) as usize;

        if bit_shift == 0 {
            // Fast path: no shifting needed
            result.extend_from_slice(&self.path[..num_path_bytes]);
        } else {
            // Shift bits right by bit_shift to align with offset
            for i in 0..num_path_bytes {
                let hi = if i == 0 { 0 } else { self.path.get(i - 1).copied().unwrap_or(0) };
                let lo = self.path.get(i).copied().unwrap_or(0);
                let byte = (hi << (8 - bit_shift)) | (lo >> bit_shift);
                result.push(byte);
            }
        }

        // Mask leading bits (before start)
        let lead_mask = 0xFFu8 >> (start % 8) as u8;
        result[4] &= lead_mask;

        // Mask trailing bits (after end)
        let trail = (end % 8) as u8;
        if trail != 0 {
            let trail_mask = 0xFFu8 << (8 - trail);
            let last_idx = result.len() - 1;
            result[last_idx] &= trail_mask;
        }

        Ok(result)
    }
}

/// Extract bits from a `Bits` into a normalized `Vec<u8>` (starting at bit 0)
#[inline]
fn extract_normalized(bits: &Bits) -> Vec<u8> {
    let len = bits.len();
    if len == 0 {
        return Vec::new();
    }

    let num_bytes = len.div_ceil(8) as usize;
    let bit_offset = (bits.range.start % 8) as usize;
    let byte_start = (bits.range.start / 8) as usize;
    let byte_end = bits.range.end.div_ceil(8) as usize;
    let src = &bits.path[byte_start..byte_end];

    let mut result = vec![0u8; num_bytes];

    if bit_offset == 0 {
        // Fast path: already byte-aligned
        result[..num_bytes.min(src.len())].copy_from_slice(&src[..num_bytes.min(src.len())]);
    } else {
        // Shift left by bit_offset to normalize
        for (i, bit) in result.iter_mut().enumerate().take(num_bytes) {
            let hi = src.get(i).copied().unwrap_or(0);
            let lo = src.get(i + 1).copied().unwrap_or(0);
            *bit = (hi << bit_offset) | (lo >> (8 - bit_offset));
        }
    }

    // Mask trailing bits
    let trail = (len % 8) as usize;
    if trail != 0 && !result.is_empty() {
        let last = result.len() - 1;
        result[last] &= 0xFF << (8 - trail);
    }

    result
}

/// Append normalized bits from src to dst at bit position dst_len
#[inline]
fn append_normalized(dst: &mut Vec<u8>, dst_len: BitsLen, src: &[u8], src_len: BitsLen) {
    if src_len == 0 {
        return
    }

    let total_len = dst_len + src_len;
    let new_bytes = total_len.div_ceil(8) as usize;
    dst.resize(new_bytes, 0);

    let bit_offset = (dst_len % 8) as usize;
    let byte_offset = (dst_len / 8) as usize;

    if bit_offset == 0 {
        // Fast path: byte-aligned append
        let copy_len = src.len().min(new_bytes - byte_offset);
        dst[byte_offset..byte_offset + copy_len].copy_from_slice(&src[..copy_len]);
    } else {
        // Shift src right by bit_offset and OR into dst
        for i in 0..src.len() {
            let shifted_hi = src[i] >> bit_offset;
            let shifted_lo = src[i] << (8 - bit_offset);

            dst[byte_offset + i] |= shifted_hi;
            if byte_offset + i + 1 < dst.len() {
                dst[byte_offset + i + 1] |= shifted_lo;
            }
        }
    }
}

/// Merge a `BitsOwned` with a `Bits`, appending b's bits after a's.
#[inline]
pub fn merge_owned_and_bits(a: &BitsOwned, b: &Bits) -> BitsOwned {
    let a_len = a.len();
    let b_len = b.len();
    let total_len = a_len + b_len;

    if total_len == 0 {
        return BitsOwned::from_raw(Vec::new(), 0, a.offset);
    }

    // Start with a copy of a's path
    let mut path = a.path.clone();

    // Extract and append b's normalized bits
    let b_norm = extract_normalized(b);
    append_normalized(&mut path, a_len, &b_norm, b_len);

    BitsOwned::from_raw(path, total_len, a.offset)
}

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

    /// Convert to `BitsOwned`, preserving the range offset.
    #[inline]
    pub fn to_bits_owned(&self) -> BitsOwned {
        let path = extract_normalized(self);
        BitsOwned::from_raw(path, self.len(), self.range.start)
    }

    /// Merge two `Bits` into a new `BitsOwned`, preserving the first's offset.
    #[inline]
    pub fn merge(a: &Self, b: &Self) -> BitsOwned {
        let a_len = a.len();
        let b_len = b.len();
        let total_len = a_len + b_len;

        if total_len == 0 {
            return BitsOwned::from_raw(Vec::new(), 0, a.range.start);
        }

        // Extract normalized bits from both
        let mut path = extract_normalized(a);
        let b_norm = extract_normalized(b);

        // Append b's bits
        append_normalized(&mut path, a_len, &b_norm, b_len);

        BitsOwned::from_raw(path, total_len, a.range.start)
    }
}
