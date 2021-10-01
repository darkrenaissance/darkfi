use log::debug;
use sha2::{Digest, Sha256};
use std::iter::FromIterator;
use std::str::FromStr;

use crate::{
    serial::{deserialize, serialize},
    util::{NetworkName, TokenList},
    Error, Result,
};

// hash the external token ID and NetworkName param.
// if fails, change the last 4 bytes and hash it again. keep repeating until it works.
pub fn generate_id(tkn_str: &str, network: &NetworkName) -> Result<jubjub::Fr> {

    let mut id_string = network.to_string();

    id_string.push_str(tkn_str);

    let mut data = bs58::decode(serialize(&id_string)).into_vec()?;

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
                debug!(target: "CASHIER", "DESERIALIZATION SUCCESSFUL");
                return Ok(token_id.unwrap());
            }
        }
    };

    Ok(token_id)
}

pub fn assign_id(network: &str, token: &str, tokenlist: TokenList) -> Result<String> {
    match NetworkName::from_str(network)? {
        NetworkName::Solana => match token.to_lowercase().as_str() {
            "solana" | "sol" => {
                let token_id = "So11111111111111111111111111111111111111112";
                Ok(token_id.to_string())
            }
            tkn => {
                // (== 44) can represent a Solana base58 token mint address
                let id = if token.len() == 44 {
                    token.to_string()
                } else {
                    symbol_to_id(tkn, tokenlist)?
                };
                Ok(id)
            }
        },
        NetworkName::Bitcoin => Err(Error::NetworkParseError),
    }
}

pub fn decimals(network: &str, token: &str, tokenlist: TokenList) -> Result<usize> {
    match NetworkName::from_str(network)? {
        NetworkName::Solana => match token {
            "solana" | "sol" => {
                let decimals = 9;
                Ok(decimals)
            }
            tkn => {
                let decimals = tokenlist.search_decimal(tkn)?;
                Ok(decimals)
            }
        },
        NetworkName::Bitcoin => Err(Error::NetworkParseError),
    }
}

//pub fn to_apo(amount: f64, decimals: u32) -> Result<u64> {
//    let apo = amount as u64 * u64::pow(10, decimals as u32);
//    Ok(apo)
//}

pub fn symbol_to_id(token: &str, tokenlist: TokenList) -> Result<String> {
    let vec: Vec<char> = token.chars().collect();
    let mut counter = 0;
    for c in vec {
        if c.is_alphabetic() {
            counter += 1;
        }
    }
    if counter == token.len() {
        tokenlist.search_id(token)
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

pub fn encode_base10(amount: u64, decimal_places: usize) -> String {
    let mut s: Vec<char> = format!("{:0width$}", amount, width = 1 + decimal_places)
        .chars()
        .collect();
    s.insert(s.len() - decimal_places, '.');

    String::from_iter(&s)
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

#[allow(unused_imports)]
mod tests {
    use crate::util::decode_base10;
    use crate::util::encode_base10;
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
