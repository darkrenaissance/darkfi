use serde_json::Value;
use std::collections::HashMap;

use crate::{
    util::{generate_id, NetworkName},
    Error, Result,
};

#[derive(Debug, Clone)]
pub struct SolTokenList {
    tokens: Vec<Value>,
}

impl SolTokenList {
    pub fn new() -> Result<Self> {
        let file_contents = include_bytes!("../../token/solanatokenlist.json");
        let sol_tokenlist: Value = serde_json::from_slice(file_contents)?;
        let tokens = sol_tokenlist["tokens"]
            .as_array()
            .ok_or(Error::TokenParseError)?
            .clone();

        Ok(Self { tokens })
    }

    pub fn get_symbols(&self) -> Result<Vec<String>> {
        let mut symbols = Vec::new();
        for item in self.tokens.iter() {
            let symbol = item["symbol"].as_str().unwrap();
            symbols.push(symbol.to_string());
        }
        return Ok(symbols);
    }

    pub fn search_id(&self, symbol: &str) -> Result<Option<String>> {
        for item in self.tokens.iter() {
            if item["symbol"] == symbol.to_uppercase() {
                let address = item["address"].clone();
                let address = address.as_str().ok_or(Error::TokenParseError)?;
                return Ok(Some(address.to_string()));
            }
        }
        Ok(None)
    }

    // pub fn search_all_id(&self, symbol: &str) -> Result<Vec<String>> {
    //     let tokens = self.sol_tokenlist["tokens"]
    //         .as_array()
    //         .ok_or(Error::TokenParseError)?;
    //     let mut ids = Vec::new();
    //     for item in tokens {
    //         if item["symbol"] == symbol.to_uppercase() {
    //             let address = item["address"].clone();
    //             let address = address.as_str().ok_or(Error::TokenParseError)?;
    //             ids.push(address.to_string());
    //         }
    //     }
    //     return Ok(ids);
    // }

    pub fn search_decimal(&self, symbol: &str) -> Result<Option<usize>> {
        for item in self.tokens.iter() {
            if item["symbol"] == symbol.to_uppercase() {
                let decimals = item["decimals"].clone();
                let decimals = decimals.as_u64().ok_or(Error::TokenParseError)?;
                let decimals = decimals as usize;
                return Ok(Some(decimals));
            }
        }
        Ok(None)
    }
}

pub struct DrkTokenList {
    pub tokens: HashMap<String, jubjub::Fr>,
}

impl DrkTokenList {
    pub fn new(sol_list: SolTokenList) -> Result<Self> {
        // get symbols
        let sol_symbols = sol_list.clone().get_symbols()?;

        let tokens: HashMap<String, jubjub::Fr> = sol_symbols
            .iter()
            .map(|sym| return (sym.clone(), generate_id(sym, &NetworkName::Solana).unwrap()))
            .collect();

        Ok(Self { tokens })
    }
}

#[allow(unused_imports)]
mod tests {
    use super::*;
    use crate::util::{DrkTokenList, SolTokenList};
    use crate::Result;

    fn _get_tokens() -> Result<SolTokenList> {
        let file_contents = include_bytes!("../../token/solanatokenlisttest.json");
        let sol_tokenlist: Value = serde_json::from_slice(file_contents)?;

        let tokens = sol_tokenlist["tokens"]
            .as_array()
            .ok_or(Error::TokenParseError)?
            .clone();

        let sol_tokenlist = SolTokenList { tokens };
        Ok(sol_tokenlist)
    }

    #[test]
    pub fn test_get_symbols() -> Result<()> {
        let tokens = _get_tokens()?;
        let symbols = tokens.get_symbols()?;
        assert_eq!(symbols.len(), 5);
        assert_eq!("MILLI", symbols[0]);
        assert_eq!("ZI", symbols[1]);
        assert_eq!("SOLA", symbols[2]);
        assert_eq!("SOL", symbols[3]);
        assert_eq!("USDC", symbols[4]);
        Ok(())
    }

    #[test]
    pub fn test_get_id_from_symbols() -> Result<()> {
        let tokens = _get_tokens()?;
        let symbol = &tokens.clone().get_symbols()?[3];
        let id = tokens.search_id(symbol)?;
        assert!(id.is_some());
        assert_eq!(id.unwrap(), "So11111111111111111111111111111111111111112");
        Ok(())
    }

    #[test]
    pub fn test_hashmap() -> Result<()> {
        let tokens = _get_tokens()?;
        let drk_token = DrkTokenList::new(tokens)?;
        assert_eq!(drk_token.tokens.len(), 5);
        assert_eq!(
            drk_token.tokens["SOL"],
            generate_id("SOL", &NetworkName::Solana)?
        );
        Ok(())
    }
}
