use crate::{
    serial::{deserialize, serialize},
    util::{NetworkName, TokenList},
    Error, Result,
};

use log::debug;
use sha2::{Digest, Sha256};
use std::str::FromStr;

//1. deposit(network, asset)
//
//internal ID = hash(externalID, NetworkName)
//deposit(internalID)

//2. withdraw(network, asset, amount)
//internal ID = hash(externalID, NetworkName)
//amountu64 = amount.to_u64()
//withdraw(internalID, amount)

//3. transfer(asset, amount, address)
//asset = { match [SOL, BTC, TOKEN]
//        return token-id from wallet.db }
//amountu64 = amount.to_u64()
//address = jubjub::SubgroupPoint
//transfer(token_id, amountu64, address)
//
//
//
//
//
// extern_tokenID
// tokenID
//

//pub fn create_id(extern_tokenID, NetworkName) -> Result<jubjub::Fr> {
//}

// DEPOSIT
// parse_network
// parse_id

// generate_id
//
// WITHDRAW
// parse_network
// parse_id
//
// generate_id
// amount.to_u64()
//
// TRANSFER
// get_id
// amount.to_u64()

// here we hash the alphanumeric token ID. if it fails, we change the last 4 bytes and hash it
// again, and keep repeating until it works.
pub fn generate_id(tkn_str: &str) -> Result<jubjub::Fr> {
    if bs58::decode(tkn_str).into_vec().is_err() {
        // TODO: make this an error
        debug!(target: "PARSE ID", "COULD NOT DECODE STR");
    }
    let mut data = bs58::decode(tkn_str).into_vec().unwrap();

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

pub fn parse_wrapped_token(token: &str, tokenlist: TokenList) -> Result<jubjub::Fr> {
    match token.to_lowercase().as_str() {
        "sol" => {
            let id = "So11111111111111111111111111111111111111112";
            let token_id = generate_id(id)?;
            Ok(token_id)
        }
        "btc" => Err(Error::TokenParseError),
        tkn => {
            // (== 44) can represent a Solana base58 token mint address
            let id = if token.len() == 44 {
                token.to_string()
            } else {
                symbol_to_id(tkn, tokenlist)?
            };

            let token_id = generate_id(&id)?;
            Ok(token_id)
        }
    }
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

pub fn parse_params(
    network: &str,
    token: &str,
    amount: u64,
    tokenlist: TokenList,
) -> Result<(String, u64)> {
    match NetworkName::from_str(network)? {
        NetworkName::Solana => match token {
            "solana" | "sol" => {
                let token_id = "So11111111111111111111111111111111111111112";
                let decimals = 9;
                let amount_in_apo = amount * u64::pow(10, decimals as u32);
                Ok((token_id.to_string(), amount_in_apo))
            }
            tkn => {
                let token_id = symbol_to_id(tkn, tokenlist.clone())?;
                let decimals = tokenlist.search_decimal(tkn)?;
                let amount_in_apo = amount * u64::pow(10, decimals as u32);
                Ok((token_id, amount_in_apo))
            }
        },
        NetworkName::Bitcoin => Err(Error::NetworkParseError),
    }
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
