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

//! Base32 encoding as specified by RFC4648
//! Optional padding is the `=` character.
// Taken from https://github.com/andreasots/base32
use core::cmp::min;

/// Standard Base32 alphabet.
const ENCODE_STD: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Encode a byte slice with the given base32 alphabet into a base32 string.
pub fn encode(padding: bool, data: &[u8]) -> String {
    let mut ret = Vec::with_capacity((data.len() + 3) / 4 * 5);

    for chunk in data.chunks(5) {
        let buf = {
            let mut buf = [0u8; 5];
            for (i, &b) in chunk.iter().enumerate() {
                buf[i] = b;
            }
            buf
        };

        ret.push(ENCODE_STD[((buf[0] & 0xf8) >> 3) as usize]);
        ret.push(ENCODE_STD[(((buf[0] & 0x07) << 2) | ((buf[1] & 0xc0) >> 6)) as usize]);
        ret.push(ENCODE_STD[((buf[1] & 0x3e) >> 1) as usize]);
        ret.push(ENCODE_STD[(((buf[1] & 0x01) << 4) | ((buf[2] & 0xf0) >> 4)) as usize]);
        ret.push(ENCODE_STD[(((buf[2] & 0x0f) << 1) | (buf[3] >> 7)) as usize]);
        ret.push(ENCODE_STD[((buf[3] & 0x7c) >> 2) as usize]);
        ret.push(ENCODE_STD[(((buf[3] & 0x03) << 3) | ((buf[4] & 0xe0) >> 5)) as usize]);
        ret.push(ENCODE_STD[(buf[4] & 0x1f) as usize]);
    }

    if data.len() % 5 != 0 {
        let len = ret.len();
        let num_extra = 8 - (data.len() % 5 * 8 + 4) / 5;
        if padding {
            for i in 1..num_extra + 1 {
                ret[len - i] = b'=';
            }
        } else {
            ret.truncate(len - num_extra);
        }
    }

    String::from_utf8(ret).unwrap()
}

const STD_INV_ALPHABET: [i8; 43] = [
    -1, -1, 26, 27, 28, 29, 30, 31, -1, -1, -1, -1, -1, 0, -1, -1, -1, 0, 1, 2, 3, 4, 5, 6, 7, 8,
    9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
];

/// Tries to decode a base32 string into a byte vector. Returns `None` if
/// something fails.
pub fn decode(data: &str) -> Option<Vec<u8>> {
    if !data.is_ascii() {
        return None
    }

    let data = data.as_bytes();
    let mut unpadded_data_len = data.len();

    for i in 1..min(6, data.len()) + 1 {
        if data[data.len() - i] != b'=' {
            break
        }
        unpadded_data_len -= 1;
    }

    let output_length = unpadded_data_len * 5 / 8;
    let mut ret = Vec::with_capacity((output_length + 4) / 5 * 5);

    for chunk in data.chunks(8) {
        let buf = {
            let mut buf = [0u8; 8];
            for (i, &c) in chunk.iter().enumerate() {
                match STD_INV_ALPHABET.get(c.to_ascii_uppercase().wrapping_sub(b'0') as usize) {
                    Some(&-1) | None => return None,
                    Some(&value) => buf[i] = value as u8,
                };
            }

            buf
        };

        ret.push((buf[0] << 3) | (buf[1] >> 2));
        ret.push((buf[1] << 6) | (buf[2] << 1) | (buf[3] >> 4));
        ret.push((buf[3] << 4) | (buf[4] >> 1));
        ret.push((buf[4] << 7) | (buf[5] << 2) | (buf[6] >> 3));
        ret.push((buf[6] << 5) | buf[7]);
    }

    ret.truncate(output_length);
    Some(ret)
}

#[cfg(test)]
mod tests {
    #[test]
    fn base32_encoding_decoding() {
        let s = b"b32Test"; // This should pad with 4 =
        let encoded = super::encode(true, &s[..]);
        assert_eq!(&encoded, "MIZTEVDFON2A====");
        assert_eq!(super::decode(&encoded).unwrap(), s);

        let s = b"b32Testoor"; // This shouldn't pad
        let encoded = super::encode(true, &s[..]);
        assert_eq!(&encoded, "MIZTEVDFON2G633S");
        assert_eq!(super::decode(&encoded).unwrap(), s);
    }
}
