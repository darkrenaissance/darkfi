use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::{
    serial::{Decodable, Encodable},
    Result,
};

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub enum NetworkName {
    Solana,
    Bitcoin,
    Ethereum,
    Empty,
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
            Self::Ethereum => {
                write!(f, "Ethereum")
            }
            Self::Empty => {
                write!(f, "No Supported Network")
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
            "eth" | "ethereum" => Ok(NetworkName::Ethereum),
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
