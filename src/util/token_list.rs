use crate::{Error, Result};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct TokenList {
    tokenlist: Value,
}

impl TokenList {
    pub fn new() -> Result<Self> {
        // TODO: FIXME
        let file_contents = std::fs::read_to_string("token/solanatokenlist.json")?;
        let tokenlist: Value = serde_json::from_str(&file_contents)?;

        Ok(Self { tokenlist })
    }

    pub fn search_id(self, symbol: &str) -> Result<String> {
        let tokens = self.tokenlist["tokens"]
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

    pub fn search_decimal(self, symbol: &str) -> Result<usize> {
        let tokens = self.tokenlist["tokens"]
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
}
