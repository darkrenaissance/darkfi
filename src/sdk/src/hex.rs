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

use crate::{util::Itertools, ContractError, GenericResult};

/// Creates a hex formatted string of the data
#[inline]
pub fn hex_from_iter<I: Iterator<Item = u8>>(iter: I) -> String {
    let mut repr = String::new();
    for b in iter {
        repr += &format!("{b:02x}");
    }
    repr
}

/// Decode hex string into bytes
pub fn decode_hex<'a>(hex: &'a str) -> HexDecodeIter<'a> {
    HexDecodeIter { hex, curr: 0 }
}

pub struct HexDecodeIter<'a> {
    hex: &'a str,
    curr: usize,
}

impl Iterator for HexDecodeIter<'_> {
    type Item = GenericResult<u8>;

    // FromIterator auto converts [Result<u8>, ...] into Result<[u8, ...]>
    // https://stackoverflow.com/a/26370894
    fn next(&mut self) -> Option<Self::Item> {
        // Stop iteration
        if self.curr == self.hex.len() {
            return None
        }

        // End of next 2 chars is past the end of the hex string
        if self.curr + 2 > self.hex.len() {
            return Some(Err(ContractError::HexFmtErr))
        }

        // Decode the next 2 chars
        let Ok(byte) = u8::from_str_radix(&self.hex[self.curr..self.curr + 2], 16) else {
            return Some(Err(ContractError::HexFmtErr))
        };

        self.curr += 2;

        Some(Ok(byte))
    }
}

pub fn decode_hex_arr<const N: usize>(hex: &str) -> GenericResult<[u8; N]> {
    let decoded: Vec<_> = decode_hex(hex).try_collect()?;
    let bytes: [u8; N] = decoded.try_into().map_err(|_| ContractError::HexFmtErr)?;
    Ok(bytes)
}

pub trait AsHex {
    fn hex(&self) -> String;
}

impl<T: AsRef<[u8]>> AsHex for T {
    /// Creates a hex formatted string of the data (big endian)
    fn hex(&self) -> String {
        hex_from_iter(self.as_ref().iter().cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_encode_decode() {
        let decoded = decode_hex("0a00").collect::<GenericResult<Vec<_>>>().unwrap();
        assert_eq!(decoded, vec![10, 0]);
        assert!(decode_hex("0x").collect::<GenericResult<Vec<_>>>().is_err());
        assert!(decode_hex("0a1").collect::<GenericResult<Vec<_>>>().is_err());
        assert_eq!(hex_from_iter([10u8, 0].into_iter()), "0a00");
    }
}
