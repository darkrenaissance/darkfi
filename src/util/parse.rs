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

use std::str::FromStr;

use crate::{Error, Result};

pub fn decode_base10(amount: &str, decimal_places: usize, strict: bool) -> Result<u64> {
    let mut s: Vec<char> = amount.to_string().chars().collect();

    // Get rid of the decimal point:
    let point: usize = if let Some(p) = amount.find('.') {
        s.remove(p);
        p
    } else {
        s.len()
    };

    // Only digits should remain
    for i in &s {
        if !i.is_ascii_digit() {
            return Err(Error::ParseFailed("Found non-digits"))
        }
    }

    // Add digits to the end if there are too few:
    let actual_places = s.len() - point;
    if actual_places < decimal_places {
        s.extend(vec!['0'; decimal_places - actual_places])
    }

    // Remove digits from the end if there are too many:
    let mut round = false;
    if actual_places > decimal_places {
        let end = point + decimal_places;
        for i in &s[end..s.len()] {
            if *i != '0' {
                round = true;
                break
            }
        }
        s.truncate(end);
    }

    if strict && round {
        return Err(Error::ParseFailed("Would end up rounding while strict"))
    }

    // Convert to an integer
    let number = u64::from_str(&String::from_iter(&s))?;

    // Round and return
    if round && number == u64::MAX {
        return Err(Error::ParseFailed("u64 overflow"))
    }

    Ok(number + round as u64)
}

pub fn encode_base10(amount: u64, decimal_places: usize) -> String {
    let mut s: Vec<char> =
        format!("{:0width$}", amount, width = 1 + decimal_places).chars().collect();
    s.insert(s.len() - decimal_places, '.');

    String::from_iter(&s).trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::{decode_base10, encode_base10};

    #[test]
    fn test_decode_base10() {
        assert_eq!(124, decode_base10("12.33", 1, false).unwrap());
        assert_eq!(1233000, decode_base10("12.33", 5, false).unwrap());
        assert_eq!(1200000, decode_base10("12.", 5, false).unwrap());
        assert_eq!(1200000, decode_base10("12", 5, false).unwrap());
        assert!(decode_base10("12.33", 1, true).is_err());
    }

    #[test]
    fn test_encode_base10() {
        assert_eq!("23.4321111", &encode_base10(234321111, 7));
        assert_eq!("23432111.1", &encode_base10(234321111, 1));
        assert_eq!("234321.1", &encode_base10(2343211, 1));
        assert_eq!("2343211", &encode_base10(2343211, 0));
        assert_eq!("0.00002343", &encode_base10(2343, 8));
    }
}
