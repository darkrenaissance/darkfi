use std::collections::HashMap;

use serde_json::Value;

use crate::{
    types::*,
    util::{generate_id, NetworkName},
    Error, Result,
};

#[derive(Debug, Clone)]
pub struct TokenList {
    tokens: Vec<Value>,
}

impl TokenList {
    pub fn new(data: &[u8]) -> Result<Self> {
        let tokenlist: Value = serde_json::from_slice(data)?;
        let tokens = tokenlist["tokens"]
            .as_array()
            .ok_or(Error::TokenParseError)?
            .clone();
        Ok(Self { tokens })
    }

    pub fn get_symbols(&self) -> Result<Vec<String>> {
        let mut symbols: Vec<String> = Vec::new();
        for item in self.tokens.iter() {
            let symbol = item["symbol"].as_str().unwrap();
            symbols.push(symbol.to_string());
        }
        Ok(symbols)
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

#[derive(Debug, Clone)]
pub struct DrkTokenList {
    pub tokens: HashMap<String, DrkTokenId>,
}

impl DrkTokenList {
    pub fn new(sol_list: TokenList) -> Result<Self> {
        let sol_symbols = sol_list.get_symbols()?;

        let mut tokens: HashMap<String, DrkTokenId> = sol_symbols
            .iter()
            .filter_map(|symbol| Self::generate_hash_pair(&sol_list, symbol).ok())
        .collect();

         tokens.insert(
            "BTC".to_string(),
            generate_id("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa", &NetworkName::Bitcoin)?,
         );

        Ok(Self { tokens })
    }

    fn generate_hash_pair(sol_list: &TokenList, symbol: &str) -> Result<(String, DrkTokenId)> {
        if let Some(token_id) = &sol_list.search_id(symbol)? {
            Ok((
                symbol.to_string(),
                generate_id(token_id, &NetworkName::Solana)?,
            ))
        } else {
            Err(Error::NotSupportedToken)
        }
    }

    pub fn symbol_from_id(&self, id: DrkTokenId) -> Result<Option<String>> {
        // TODO:
        /*
        if id.to_string() == "0x01300f9bce0f9ba7168dc001a67bcbda3a5bf4bdb4c56ae900fe4698cee9a7bd" {
            return Ok(Some("BTC".to_string()));
        }
        */

        Ok(self
            .tokens
            .iter()
            .find_map(|(key, &val)| if val == id { Some(key.clone()) } else { None }))
    }
}

#[allow(unused_imports)]
mod tests {
    use super::*;
    use crate::util::{DrkTokenList, TokenList};
    use crate::Result;

    fn _get_sol_tokens() -> Result<TokenList> {
        let file_contents = include_bytes!("../../testdata/solanatokenlisttest.json");
        let sol_tokenlist: Value = serde_json::from_slice(file_contents)?;

        let tokens = sol_tokenlist["tokens"]
            .as_array()
            .ok_or(Error::TokenParseError)?
            .clone();

        let sol_tokenlist = TokenList { tokens };
        Ok(sol_tokenlist)
    }

    fn _get_eth_tokens() -> Result<TokenList> {
        let file_contents = include_bytes!("../../testdata/erc20tokenlisttest.json");
        let eth_tokenlist: Value = serde_json::from_slice(file_contents)?;

        let tokens = eth_tokenlist["tokens"]
            .as_array()
            .ok_or(Error::TokenParseError)?
            .clone();

        let eth_tokenlist = TokenList { tokens };
        Ok(eth_tokenlist)
    }

    fn _get_btc_tokens() -> Result<TokenList> {
        let file_contents = include_bytes!("../../token/bitcoin_token_list.json");
        let btc_tokenlist: Value = serde_json::from_slice(file_contents)?;

        let tokens = btc_tokenlist["tokens"]
            .as_array()
            .ok_or(Error::TokenParseError)?
            .clone();

        let btc_tokenlist = TokenList { tokens };
        Ok(btc_tokenlist)
    }

    #[test]
    pub fn test_get_symbols() -> Result<()> {
        let tokens = _get_sol_tokens()?;
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
        let tokens = _get_sol_tokens()?;
        let symbol = &tokens.get_symbols()?[3];
        let id = tokens.search_id(symbol)?;
        assert!(id.is_some());
        assert_eq!(id.unwrap(), "So11111111111111111111111111111111111111112");
        Ok(())
    }

    #[test]
    pub fn test_hashmap() -> Result<()> {
        let sol_tokens = _get_sol_tokens()?;
        let sol_tokens2 = _get_sol_tokens()?;
        let eth_tokens = _get_eth_tokens()?;
        let eth_tokens2 = _get_eth_tokens()?;
        let btc_tokens = _get_btc_tokens()?;
        let btc_tokens2 = _get_btc_tokens()?;

        let drk_token = DrkTokenList::new(&sol_tokens, &eth_tokens, &btc_tokens)?;

        assert_eq!(drk_token.tokens[&NetworkName::Solana].len(), 5);
        assert_eq!(drk_token.tokens[&NetworkName::Ethereum].len(), 3);
        assert_eq!(drk_token.tokens[&NetworkName::Bitcoin].len(), 1);

        assert_eq!(
            drk_token.tokens[&NetworkName::Solana]["SOL"],
            generate_id(
                &sol_tokens2.search_id("SOL")?.unwrap(),
                &NetworkName::Solana
            )?
        );

        assert_eq!(
            drk_token.tokens[&NetworkName::Bitcoin]["BTC"],
            generate_id(
                &btc_tokens2.search_id("BTC")?.unwrap(),
                &NetworkName::Bitcoin
            )?
        );

        assert_eq!(
            drk_token.tokens[&NetworkName::Ethereum]["WBTC"],
            generate_id(
                &eth_tokens2.search_id("WBTC")?.unwrap(),
                &NetworkName::Ethereum
            )?
        );

        Ok(())
    }
}
