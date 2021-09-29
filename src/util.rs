use crate::{
    serial::{deserialize, serialize, Decodable, Encodable},
    Error, Result,
};

use log::debug;
use sha2::{Digest, Sha256};

use std::path::{Path, PathBuf};
use std::str::FromStr;

pub fn expand_path(path: &str) -> Result<PathBuf> {
    let ret: PathBuf;

    if path.starts_with("~/") {
        let homedir = dirs::home_dir().unwrap();
        let remains = PathBuf::from(path.strip_prefix("~/").unwrap());
        ret = [homedir, remains].iter().collect();
    } else if path.starts_with('~') {
        ret = dirs::home_dir().unwrap();
    } else {
        ret = PathBuf::from(path);
    }

    Ok(ret)
}

pub fn join_config_path(file: &Path) -> Result<PathBuf> {
    let mut path = PathBuf::new();
    let dfi_path = Path::new("darkfi");

    if let Some(v) = dirs::config_dir() {
        path.push(v);
    }

    path.push(dfi_path);
    path.push(file);

    Ok(path)
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum NetworkName {
    Solana,
    Bitcoin,
}

impl std::fmt::Display for NetworkName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Solana => {
                write!(f, "Solana")
            }
            Self::Bitcoin => {
                write!(f, "Bitcoin")
            }
        }
    }
}

impl FromStr for NetworkName {
    type Err = crate::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "sol" | "solana" => Ok(NetworkName::Solana),
            "btc" | "bitcoin" => Ok(NetworkName::Bitcoin),
            _ => Err(crate::Error::NotSupportedNetwork),
        }
    }
}

impl Encodable for NetworkName {
    fn encode<S: std::io::Write>(&self, s: S) -> Result<usize> {
        let name = self.to_string();
        let len = name.encode(s)?;
        Ok(len)
    }
}

impl Decodable for NetworkName {
    fn decode<D: std::io::Read>(mut d: D) -> Result<Self> {
        let name: String = Decodable::decode(&mut d)?;
        let name = NetworkName::from_str(&name)?;
        Ok(name)
    }
}

// here we hash the alphanumeric token ID. if it fails, we change the last 4 bytes and hash it
// again, and keep repeating until it works.
pub fn generate_id(tkn_str: &str) -> Result<jubjub::Fr> {
    if bs58::decode(tkn_str).into_vec().is_err() {
        // TODO: make this an error
        debug!(target: "PARSE ID", "COULD NOT DECODE STR");
    }
    let mut data = bs58::decode(tkn_str).into_vec().unwrap();
    let token_id = deserialize::<jubjub::Fr>(&data);
    if token_id.is_err() {
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
            let tkn = token_id.unwrap();
            return Ok(tkn);
        }
    } else {
        Ok(token_id.unwrap())
    }
}

pub fn parse_wrapped_token(token: &str) -> Result<jubjub::Fr> {
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
                symbol_to_id(tkn)?
            };

            let token_id = generate_id(&id)?;
            Ok(token_id)
        }
    }
}

pub fn parse_network(network: &str, token: &str) -> Result<String> {
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
                    symbol_to_id(tkn)?
                };
                Ok(id)
            }
        },
        NetworkName::Bitcoin => Err(Error::NetworkParseError),
    }
}

pub fn parse_params(network: &str, token: &str, amount: u64) -> Result<(String, u64)> {
    match NetworkName::from_str(network)? {
        NetworkName::Solana => match token {
            "solana" | "sol" => {
                let token_id = "So11111111111111111111111111111111111111112";
                let decimals = 9;
                let amount_in_apo = amount * u64::pow(10, decimals as u32);
                Ok((token_id.to_string(), amount_in_apo))
            }
            tkn => {
                let token_id = symbol_to_id(tkn)?;
                let decimals = search_decimal(tkn)?;
                let amount_in_apo = amount * u64::pow(10, decimals as u32);
                Ok((token_id, amount_in_apo))
            }
        },
        NetworkName::Bitcoin => Err(Error::NetworkParseError),
    }
}

pub fn symbol_to_id(token: &str) -> Result<String> {
    let vec: Vec<char> = token.chars().collect();
    let mut counter = 0;
    for c in vec {
        if c.is_alphabetic() {
            counter += 1;
        }
    }
    if counter == token.len() {
        search_id(token)
    } else {
        Ok(token.to_string())
    }
}

pub fn search_id(symbol: &str) -> Result<String> {
    // TODO: FIXME
    let file_contents = std::fs::read_to_string("token/solanatokenlist.json")?;
    let tokenlist: serde_json::Value = serde_json::from_str(&file_contents)?;
    let tokens = tokenlist["tokens"]
        .as_array()
        .ok_or(Error::TokenParseError)?;
    for item in tokens {
        if item["symbol"] == symbol.to_uppercase() {
            let address = item["address"].clone();
            let address = address.as_str().ok_or(Error::TokenParseError)?;
            return Ok(address.to_string());
        }
    }
    unreachable!();
}

pub fn search_decimal(symbol: &str) -> Result<u64> {
    // TODO: FIXME
    let file_contents = std::fs::read_to_string("token/solanatokenlist.json")?;
    let tokenlist: serde_json::Value = serde_json::from_str(&file_contents)?;
    let tokens = tokenlist["tokens"]
        .as_array()
        .ok_or(Error::TokenParseError)?;
    for item in tokens {
        if item["symbol"] == symbol.to_uppercase() {
            let decimals = item["decimals"].clone();
            let decimals = decimals.as_u64().ok_or(Error::TokenParseError)?;
            return Ok(decimals);
        }
    }
    unreachable!();
}

#[cfg(test)]
mod tests {
    use crate::serial::{deserialize, serialize};
    use sha2::{Digest, Sha256};

    #[test]
    fn test_jubjub_parsing() {
        // 1. counter = 0
        // 2. serialized_counter = serialize(counter)
        // 3. asset_id_data = hash(data + serialized_counter)
        // 4. asset_id = deserialize(asset_id_data)
        // 5. test parse
        // 6. loop
        let tkn_str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
        println!("{}", tkn_str);
        if bs58::decode(tkn_str).into_vec().is_err() {
            println!("Could not decode str into vec");
        }
        let mut data = bs58::decode(tkn_str).into_vec().unwrap();
        println!("{:?}", data);
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let hash = hasher.finalize();
        let token_id = deserialize::<jubjub::Fr>(&hash);
        println!("{:?}", token_id);
        let mut counter = 0;
        if token_id.is_err() {
            println!("could not deserialize tkn 58");
            loop {
                println!("TOKEN IS NONE. COMMENCING LOOP");
                counter += 1;
                println!("LOOP NUMBER {}", counter);
                println!("{:?}", data.len());
                data.truncate(28);
                let serialized_counter = serialize(&counter);
                println!("{:?}", serialized_counter);
                data.extend(serialized_counter.iter());
                println!("{:?}", data.len());
                let mut hasher = Sha256::new();
                hasher.update(&data);
                let hash = hasher.finalize();
                let token_id = deserialize::<jubjub::Fr>(&hash);
                println!("{:?}", token_id);
                if token_id.is_err() {
                    continue;
                }
                if counter > 10 {
                    break;
                }
                println!("deserialization successful");
                token_id.unwrap();
                break;
            }
        };
    }
}
