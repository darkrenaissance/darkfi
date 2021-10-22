use std::iter::FromIterator;
use std::str::FromStr;

use num_bigint::BigUint;
use sha2::{Digest, Sha256};

use crate::{
    serial::{deserialize, serialize},
    util::{NetworkName, SolTokenList},
    Error, Result,
};

// hash the external token ID and NetworkName param.
// if fails, change the last 4 bytes and hash it again. keep repeating until it works.
pub fn generate_id(tkn_str: &str, network: &NetworkName) -> Result<jubjub::Fr> {
    let mut id_string = network.to_string();

    id_string.push_str(tkn_str);

    let mut data: Vec<u8> = serialize(&id_string);

    let token_id = match deserialize::<jubjub::Fr>(&data) {
        Ok(v) => v,
        Err(_) => {
            let mut counter = 0;
            loop {
                data.truncate(28);
                let serialized_counter = serialize(&counter);
                data.extend(serialized_counter.iter());
                let mut hasher = Sha256::new();
                hasher.update(&data);
                let hash = hasher.finalize();
                let token_id = deserialize::<jubjub::Fr>(&hash);
                if token_id.is_err() {
                    counter += 1;
                    continue;
                }
                return Ok(token_id.unwrap());
            }
        }
    };

    Ok(token_id)
}

pub fn assign_id(network: &str, token: &str, _tokenlist: &SolTokenList) -> Result<String> {
    let token = token.to_lowercase();
    let _token = token.as_str();
    match NetworkName::from_str(network)? {
        #[cfg(feature = "sol")]
        NetworkName::Solana => {
            // (== 44) can represent a Solana base58 token mint address
            let id = if _token.len() == 44 {
                _token.to_string()
            } else {
                symbol_to_id(_token, _tokenlist)?
            };
            Ok(id)
        }
        #[cfg(feature = "btc")]
        NetworkName::Bitcoin => {
            let id = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".to_string();
            Ok(id)
        }
        _ => Err(Error::NotSupportedNetwork),
    }
}

pub fn symbol_to_id(token: &str, tokenlist: &SolTokenList) -> Result<String> {
    let vec: Vec<char> = token.chars().collect();
    let mut counter = 0;
    for c in vec {
        if c.is_alphabetic() {
            counter += 1;
        }
    }
    if counter == token.len() {
        if let Some(id) = tokenlist.search_id(token)? {
            Ok(id)
        } else {
            Err(Error::TokenParseError)
        }
    } else {
        Ok(token.to_string())
    }
}

fn is_digit(c: char) -> bool {
    ('0'..='9').contains(&c)
}

fn char_eq(a: char, b: char) -> bool {
    a == b
}

pub fn decode_base10(amount: &str, decimal_places: usize, strict: bool) -> Result<u64> {
    let mut s: Vec<char> = amount.to_string().chars().collect();

    // Get rid of the decimal point:
    let point: usize;
    if let Some(p) = amount.find('.') {
        s.remove(p);
        point = p;
    } else {
        point = s.len();
    }

    // Only digits should remain
    for i in &s {
        if !is_digit(*i) {
            return Err(Error::ParseFailed("Found non-digits"));
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
                break;
            }
        }
        s.truncate(end);
    }

    if strict && round {
        return Err(Error::ParseFailed("Would end up rounding while strict"));
    }

    // Convert to an integer
    let number = u64::from_str(&String::from_iter(&s))?;

    // Round and return
    if round && number == u64::MAX {
        return Err(Error::ParseFailed("u64 overflow"));
    }

    Ok(number + round as u64)
}

pub fn encode_base10(amount: BigUint, decimal_places: usize) -> String {
    let mut s: Vec<char> = format!("{:0width$}", amount, width = 1 + decimal_places)
        .chars()
        .collect();
    s.insert(s.len() - decimal_places, '.');

    String::from_iter(&s)
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

pub fn truncate(amount: u64, decimals: u16, token_decimals: u16) -> Result<u64> {
    let mut amount: Vec<char> = amount.to_string().chars().collect();

    if token_decimals > decimals {
        if amount.len() <= (token_decimals - decimals) as usize {
            return Ok(0);
        }
        amount.truncate(amount.len() - (token_decimals - decimals) as usize);
    }

    if token_decimals < decimals {
        amount.resize(amount.len() + (decimals - token_decimals) as usize, '0');
    }

    let amount = u64::from_str(&String::from_iter(amount))?;
    Ok(amount)
}

#[allow(unused_imports)]
mod tests {
    use num_bigint::ToBigUint;
    
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
        assert_eq!(
            "23.4321111",
            &encode_base10((234321111 as u64).to_biguint().unwrap(), 7)
        );
        assert_eq!(
            "23432111.1",
            &encode_base10((234321111 as u64).to_biguint().unwrap(), 1)
        );
        assert_eq!("234321.1", &encode_base10((2343211 as u64).to_biguint().unwrap(), 1));
        assert_eq!("2343211", &encode_base10((2343211 as u64).to_biguint().unwrap(), 0));
        assert_eq!("0.00002343", &encode_base10((2343 as u64).to_biguint().unwrap(), 8));
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
