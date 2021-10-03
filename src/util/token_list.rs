use crate::{
    util::{generate_id, NetworkName},
    Error, Result,
};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SolTokenList {
    sol_tokenlist: Value,
}

impl SolTokenList {
    pub fn new() -> Result<Self> {
        // TODO: FIXME
        let file_contents = std::fs::read_to_string("token/solanatokenlist.json")?;
        let sol_tokenlist: Value = serde_json::from_str(&file_contents)?;

        let tokens = sol_tokenlist["tokens"]
            .as_array()
            .ok_or(Error::TokenParseError)?;
        let mut symbols = Vec::new();
        for item in tokens {
            let symbol = item["symbol"].as_str().unwrap();
            symbols.push(symbol.to_string());
        }

        Ok(Self { sol_tokenlist })
    }

    pub fn get_symbols(self) -> Result<Vec<String>> {
        let tokens = self.sol_tokenlist["tokens"]
            .as_array()
            .ok_or(Error::TokenParseError)?;
        let mut symbols = Vec::new();
        for item in tokens {
            let symbol = item["symbol"].as_str().unwrap();
            symbols.push(symbol.to_string());
        }
        return Ok(symbols);
    }

    pub fn search_id(&self, symbol: &str) -> Result<String> {
        let tokens = self.sol_tokenlist["tokens"]
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

    pub fn search_all_id(&self, symbol: &str) -> Result<Vec<String>> {
        let tokens = self.sol_tokenlist["tokens"]
            .as_array()
            .ok_or(Error::TokenParseError)?;
        let mut ids = Vec::new();
        for item in tokens {
            if item["symbol"] == symbol.to_uppercase() {
                let address = item["address"].clone();
                let address = address.as_str().ok_or(Error::TokenParseError)?;
                ids.push(address.to_string());
            }
        }
        return Ok(ids);
    }

    pub fn search_decimal(self, symbol: &str) -> Result<usize> {
        let tokens = self.sol_tokenlist["tokens"]
            .as_array()
            .ok_or(Error::TokenParseError)?;
        for item in tokens {
            if item["symbol"] == symbol.to_uppercase() {
                let decimals = item["decimals"].clone();
                let decimals = decimals.as_u64().ok_or(Error::TokenParseError)?;
                let decimals = decimals as usize;
                return Ok(decimals);
            }
        }
        unreachable!();
    }
}

pub struct DrkTokenList {
    drk_tokenlist: HashMap<String, jubjub::Fr>,
}

impl DrkTokenList {
    pub fn new(list: SolTokenList) -> Result<Self> {
        let mut drk_tokenlist = HashMap::new();
        let symbols = list.clone().get_symbols()?;

        for symbol in symbols {
            let id = list.clone().search_id(&symbol)?;
            let drk_id = generate_id(&id, &NetworkName::Solana)?;
            drk_tokenlist.insert(symbol, drk_id);
        }
        //TODO: add btc_id, NetworkName::Bitcoin to drk_tokenlist
        Ok(Self { drk_tokenlist })
    }
}

mod tests {

    use super::*;
    use crate::util::SolTokenList;
    use crate::Result;

    #[test]
    pub fn test_get_symbols() -> Result<()> {
        let token = SolTokenList::new()?;
        let symbols = token.get_symbols()?;
        for symbol in symbols {
            println!("{}", symbol)
        }
        Ok(())
    }
    #[test]
    pub fn test_get_id_from_symbols() -> Result<()> {
        let token = SolTokenList::new()?;
        let symbols = token.clone().get_symbols()?;
        for symbol in symbols {
            token.clone().search_all_id(&symbol)?;
        }
        Ok(())
    }
}
