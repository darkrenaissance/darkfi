use crate::{
    serial::{deserialize, serialize},
    util::{NetworkName, TokenList},
    Error, Result,
};

use log::debug;
use sha2::{Digest, Sha256};
use std::str::FromStr;

// hash the external token ID and NetworkName param.
// if fails, change the last 4 bytes and hash it again. keep repeating until it works.
pub fn generate_id(tkn_str: &str, network: &NetworkName) -> Result<jubjub::Fr> {
    let mut id_string = network.to_string();
    id_string.push_str(tkn_str);
    if bs58::decode(id_string.clone()).into_vec().is_err() {
        // TODO: make this an error
        debug!(target: "PARSE ID", "COULD NOT DECODE STR");
    }
    let mut data = bs58::decode(id_string).into_vec().unwrap();

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

pub fn decimals(network: &str, token: &str, tokenlist: TokenList) -> Result<u64> {
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

pub fn to_apo(amount: f64, decimals: u32) -> Result<u64> {
    let apo = amount as u64 * u64::pow(10, decimals as u32);
    Ok(apo)
}

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

mod tests {
    #[test]
    fn decode_base10() {
        const RADIX: u32 = 10;
        // TODO: this number varies per token
        let decimal_places = 10;

        let input = "0.1111";
        println!("Initial input: {}", input);
        let mut input_str = input.to_string();

        // remove the decimal point
        let mut amount: String = match input_str.find(".") {
            Some(v) => {
                input_str.remove(v);
                input_str
            }
            None => {
                print!("Number isn't a float");
                input_str
            }
        };

        println!("Removed decimal point: {}", amount);
        // only digits should remain:
        for c in amount.chars() {
            if c.is_digit(RADIX) == false {
                println!("Amount is not valid digits!")
            }
        }

        // add digits to the end if there are too few
        if amount.len() < decimal_places {
            loop {
                amount.push('0');

                if amount.len() == decimal_places {
                    break;
                }
                continue;
            }
        }

        // remove digits from the end if there are too many
        if amount.len() > decimal_places {
            loop {
                amount.pop();

                if amount.len() == decimal_places {
                    break;
                }
                continue;
            }
        }

        println!("Resized amount: {}", amount);

        // convert to an integer
        for i in amount.chars() {
            let digit = i.to_digit(RADIX).unwrap();
            println!("Converted to integer: {}", digit);
        }
    }

    #[test]
    fn encode_base10() {
        let input = 1000000000;
        println!("Original input: {}", input);
        let mut input_str = input.to_string();

        input_str.insert(1, '.');
        let amount = input_str.trim();
        println!("Encoded output: {}", amount);
    }
}
