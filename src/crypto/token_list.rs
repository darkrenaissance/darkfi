use std::str::FromStr;

use fxhash::FxHashMap;
use pasta_curves::group::ff::PrimeField;
use serde_json::Value;

use super::{token_id::generate_id, types::DrkTokenId};
use crate::{util::net_name::NetworkName, Result};

#[derive(Clone, Debug)]
pub struct TokenInfo {
    pub net_address: String,
    pub drk_address: DrkTokenId,
    pub name: String,
    pub decimals: u64,
}

#[derive(Clone)]
pub struct TokenList(pub FxHashMap<String, TokenInfo>);

impl TokenList {
    /// Create a new `TokenList` given a standard JSON object (as bytes)
    pub fn new(network_name: &str, data: &[u8]) -> Result<Self> {
        let tokenlist: Value = serde_json::from_slice(data)?;

        let mut map = FxHashMap::default();
        for i in tokenlist["tokens"].as_array().unwrap() {
            let net_address = i["address"].as_str().unwrap().to_string();
            let decimals = i["decimals"].as_u64().unwrap();
            let name = i["name"].as_str().unwrap().to_string();
            let drk_address = generate_id(&NetworkName::from_str(network_name)?, &net_address)?;

            let info = TokenInfo { net_address, drk_address, decimals, name };
            let ticker = i["symbol"].as_str().unwrap().to_uppercase().to_string();
            map.insert(ticker, info);
        }

        Ok(Self(map))
    }

    /// Tries to find the address and name of a given ticker in
    /// the hashmap.
    pub fn get(&self, ticker: String) -> Option<TokenInfo> {
        if let Some(info) = self.0.get(&ticker) {
            return Some(info.clone())
        }

        None
    }
}

#[derive(Clone)]
pub struct DrkTokenList {
    pub by_net: FxHashMap<NetworkName, TokenList>,
    pub by_addr: FxHashMap<String, (NetworkName, TokenInfo)>,
}

impl DrkTokenList {
    pub fn new(data: &[(&str, &[u8])]) -> Result<Self> {
        let mut by_net = FxHashMap::default();
        let mut by_addr = FxHashMap::default();

        for (name, json) in data {
            let net_name = NetworkName::from_str(name)?;
            let tokenlist = TokenList::new(name, json)?;
            for (_, token) in tokenlist.0.iter() {
                by_addr.insert(
                    bs58::encode(token.drk_address.to_repr()).into_string(),
                    (net_name.clone(), token.clone()),
                );
            }
            by_net.insert(net_name, tokenlist);
        }

        Ok(Self { by_net, by_addr })
    }
}
