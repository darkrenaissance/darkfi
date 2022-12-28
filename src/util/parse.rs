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

use std::{iter::FromIterator, str::FromStr};

use crate::{Error, Result};

fn is_digit(c: char) -> bool {
    ('0'..='9').contains(&c)
}

fn char_eq(a: char, b: char) -> bool {
    a == b
}

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
        if !is_digit(*i) {
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
            if !char_eq(*i, '0') {
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

pub fn truncate(amount: u64, decimals: u16, token_decimals: u16) -> Result<u64> {
    let mut amount: Vec<char> = amount.to_string().chars().collect();

    if token_decimals > decimals {
        if amount.len() <= (token_decimals - decimals) as usize {
            return Ok(0)
        }
        amount.truncate(amount.len() - (token_decimals - decimals) as usize);
    }

    if token_decimals < decimals {
        amount.resize(amount.len() + (decimals - token_decimals) as usize, '0');
    }

    let amount = u64::from_str(&String::from_iter(amount))?;
    Ok(amount)
}

#[cfg(test)]
mod tests {
    use super::{decode_base10, encode_base10, truncate};

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

    #[test]
    fn test_truncate() {
        // Token decimals is equal to 8
        assert_eq!(100, truncate(100, 8, 8).unwrap());
        assert_eq!(12, truncate(12, 8, 8).unwrap());

        // Token decimals is bigger than 8
        assert_eq!(100000000, truncate(1000000000, 8, 9).unwrap());
        assert_eq!(10, truncate(100, 8, 9).unwrap());
        assert_eq!(1, truncate(12, 8, 9).unwrap());
        assert_eq!(10, truncate(102, 8, 9).unwrap());
        assert_eq!(0, truncate(1, 8, 9).unwrap());
        assert_eq!(1, truncate(100000000, 8, 16).unwrap());
        assert_eq!(10, truncate(100000000, 8, 15).unwrap());
        assert_eq!(0, truncate(100000000, 8, 17).unwrap());
        assert_eq!(0, truncate(10, 8, 16).unwrap());

        // Token decimals is less than 8
        assert_eq!(1000, truncate(100, 8, 7).unwrap());
        assert_eq!(12000, truncate(120, 8, 6).unwrap());
        assert_eq!(1000000, truncate(100, 8, 4).unwrap());

        // token decimals is 0
        assert_eq!(00000000, truncate(0, 8, 0).unwrap());
        assert_eq!(100000000, truncate(1, 8, 0).unwrap());

        //
        // reverse truncate
        //

        // Token decimals is less than decimals
        assert_eq!(1000000000, truncate(100000000, 9, 8).unwrap());
        assert_eq!(100000000, truncate(10000000, 9, 8).unwrap());
        assert_eq!(100, truncate(10, 9, 8).unwrap());
        assert_eq!(10, truncate(1, 9, 8).unwrap());
        assert_eq!(100, truncate(10, 9, 8).unwrap());
        assert_eq!(0, truncate(0, 9, 8).unwrap());
        assert_eq!(100000000, truncate(1, 16, 8).unwrap());
        assert_eq!(100000000, truncate(10, 15, 8).unwrap());
        assert_eq!(0, truncate(0, 17, 8).unwrap());

        // Token decimals is bigger than decimals
        assert_eq!(100, truncate(1000, 7, 8).unwrap());
        assert_eq!(120, truncate(12000, 6, 8).unwrap());
        assert_eq!(100, truncate(1000000, 4, 8).unwrap());

        // token decimals is 0
        assert_eq!(0, truncate(00000000, 0, 8).unwrap());
        assert_eq!(1, truncate(100000000, 0, 8).unwrap());
    }
}
