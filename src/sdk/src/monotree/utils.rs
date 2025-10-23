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

use std::{cmp, ops::Range};

use num::{NumCast, PrimInt};
use rand::Rng;

use super::{Hash, HASH_LEN};

#[macro_export]
/// std::cmp::max() extension for use with multiple arguments.
macro_rules! max {
    ($x:expr) => ($x);
    ($x:expr, $($e:expr),+) => (cmp::max($x, max!($($e),+)));
}

#[macro_export]
/// std::cmp::min() extension for use with multiple arguments.
macro_rules! min {
    ($x:expr) => ($x);
    ($x:expr, $($e:expr),+) => (cmp::min($x, min!($($e),+)));
}

/// Cast from a typed scalar to another based on `num_traits`
pub fn cast<T: NumCast, U: NumCast>(n: T) -> U {
    NumCast::from(n).expect("cast(): Numcast")
}

/// Generate a random byte based on `rand::random`.
pub fn random_byte() -> u8 {
    rand::random::<u8>()
}

/// Generate random bytes of the given length.
pub fn random_bytes(n: usize) -> Vec<u8> {
    (0..n).map(|_| random_byte()).collect()
}

/// Generate a random `Hash`, byte-array of `HASH_LEN` length.
pub fn random_hash() -> Hash {
    slice_to_hash(&random_bytes(HASH_LEN))
}

/// Generate a vector of random `Hash` with the given length.
pub fn random_hashes(n: usize) -> Vec<Hash> {
    (0..n).map(|_| random_hash()).collect()
}

/// Get a fixed length byte-array or `Hash` from slice.
pub fn slice_to_hash(slice: &[u8]) -> Hash {
    let mut hash = [0x00; HASH_LEN];
    hash.copy_from_slice(slice);
    hash
}

/// Shuffle a slice using _Fisher-Yates_ algorithm.
pub fn shuffle<T: Clone>(slice: &mut [T]) {
    let mut rng = rand::thread_rng();
    let s = slice.len();
    (0..s).for_each(|i| {
        let q = rng.gen_range(0..s);
        slice.swap(i, q);
    });
}

/// Get sorted indices from unsorted slice.
pub fn get_sorted_indices<T>(slice: &[T], reverse: bool) -> Vec<usize>
where
    T: Clone + cmp::Ord,
{
    let mut t: Vec<_> = slice.iter().enumerate().collect();

    if reverse {
        t.sort_unstable_by(|(_, a), (_, b)| b.cmp(a));
    } else {
        t.sort_unstable_by(|(_, a), (_, b)| a.cmp(b));
    }

    t.iter().map(|(i, _)| *i).collect()
}

/// Get length of the longest common prefix bits for the given two slices.
pub fn len_lcp<T>(a: &[u8], m: &Range<T>, b: &[u8], n: &Range<T>) -> T
where
    T: PrimInt + NumCast,
    Range<T>: Iterator<Item = T>,
{
    let count = (cast(0)..min!(m.end - m.start, n.end - n.start))
        .take_while(|&i| bit(a, m.start + i) == bit(b, n.start + i))
        .count();
    cast(count)
}

static BIT_MASKS: [u8; 8] = [0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01];

/// Get `i`-th bit from bytes slice. Index `i` starts from 0.
#[inline]
pub fn bit<T: PrimInt + NumCast>(bytes: &[u8], i: T) -> bool {
    let i_usize = i.to_usize().unwrap();
    let q = i_usize >> 3;
    let r = i_usize & 7;
    if q >= bytes.len() {
        return false;
    }
    bytes[q] & BIT_MASKS[r] != 0
}

/// Get the required length of bytes from a `Range`, bits indices across the bytes.
pub fn nbytes_across<T: PrimInt + NumCast>(start: T, end: T) -> T {
    let eight = cast(8);
    let bits = end - (start - start % eight);
    (bits + eight - T::one()) / eight
}

/// Convert big-endian bytes into base10 or decimal number.
pub fn bytes_to_int<T: PrimInt + NumCast>(bytes: &[u8]) -> T {
    let l = bytes.len();
    let sum = (0..l).fold(0, |sum, i| sum + (1 << ((l - i - 1) * 8)) * bytes[i] as usize);
    cast(sum)
}

/// Get a compressed bytes (leading-zero-truncated big-endian bytes) from a `u64`.
pub fn int_to_bytes(number: u64) -> Vec<u8> {
    match number {
        0 => vec![0x00],
        _ => number.to_be_bytes().iter().skip_while(|&x| *x == 0x00).copied().collect(),
    }
}

/// Convert a Vec slice of bit or `bool` into a number as `usize`.
pub fn bits_to_usize(bits: &[bool]) -> usize {
    let l = bits.len();
    (0..l).fold(0, |sum, i| sum + ((bits[i] as usize) << (l - 1 - i)))
}

/// Convert a bytes slice into a Vec of bit.
pub fn bytes_to_bits(bytes: &[u8]) -> Vec<bool> {
    bytes_to_slicebit(bytes, &(0..bytes.len() * 8))
}

/// Convert (bytes slice + Range) representation into bits in forms of `Vec<bool>`.
pub fn bytes_to_slicebit<T>(bytes: &[u8], range: &Range<T>) -> Vec<bool>
where
    T: PrimInt + NumCast,
    Range<T>: Iterator<Item = T>,
{
    range.clone().map(|x| bit(bytes, x)).collect()
}

/// Convert bits, Vec slice of `bool` into bytes, `Vec<u8>`.
pub fn bits_to_bytes(bits: &[bool]) -> Vec<u8> {
    bits.rchunks(8).rev().map(|v| bits_to_usize(v) as u8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_bit() {
        let bytes = [0x73, 0x6f, 0x66, 0x69, 0x61];
        assert!(bit(&bytes, 10));
        assert!(!bit(&bytes, 20));
        assert!(!bit(&bytes, 30));
    }

    #[test]
    fn test_nbyte_across() {
        assert_eq!(nbytes_across(0, 8), 1);
        assert_eq!(nbytes_across(1, 7), 1);
        assert_eq!(nbytes_across(5, 9), 2);
        assert_eq!(nbytes_across(9, 16), 1);
        assert_eq!(nbytes_across(7, 19), 3);
    }

    #[test]
    fn test_bytes_to_int() {
        let number: usize = bytes_to_int(&[0x73, 0x6f, 0x66, 0x69, 0x61]);
        assert_eq!(number, 495790221665usize);
    }

    #[test]
    fn test_usize_to_bytes() {
        assert_eq!(int_to_bytes(495790221665u64), [0x73, 0x6f, 0x66, 0x69, 0x61]);
    }

    #[test]
    fn test_bytes_to_bits() {
        assert_eq!(
            bytes_to_bits(&[0x33, 0x33]),
            [
                false, false, true, true, false, false, true, true, false, false, true, true,
                false, false, true, true,
            ]
        );
    }

    #[test]
    fn test_bits_to_bytes() {
        let bits = [
            false, false, true, true, false, false, true, true, false, false, true, true, false,
            false, true, true,
        ];
        assert_eq!(bits_to_bytes(&bits), [0x33, 0x33]);
    }

    #[test]
    fn test_bits_to_usize() {
        assert_eq!(
            bits_to_usize(&[
                false, false, true, true, false, false, true, true, false, false, true, true,
                false, false, true, true,
            ]),
            13107usize
        );
    }

    #[test]
    fn test_len_lcp() {
        let sofia = [0x73, 0x6f, 0x66, 0x69, 0x61];
        let maria = [0x6d, 0x61, 0x72, 0x69, 0x61];
        assert_eq!(len_lcp(&sofia, &(0..3), &maria, &(0..3)), 3);
        assert_eq!(len_lcp(&sofia, &(0..3), &maria, &(5..9)), 0);
        assert_eq!(len_lcp(&sofia, &(2..9), &maria, &(18..30)), 5);
        assert_eq!(len_lcp(&sofia, &(20..30), &maria, &(3..15)), 4);
    }
}
