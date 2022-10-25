use std::{io, str::FromStr};

use darkfi_serial::{Decodable, Encodable};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub enum NetworkName {
    DarkFi,
    Solana,
    Bitcoin,
    Ethereum,
}

impl core::fmt::Display for NetworkName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DarkFi => {
                write!(f, "DarkFi")
            }
            Self::Solana => {
                write!(f, "Solana")
            }
            Self::Bitcoin => {
                write!(f, "Bitcoin")
            }
            Self::Ethereum => {
                write!(f, "Ethereum")
            }
        }
    }
}

impl FromStr for NetworkName {
    type Err = crate::Error;

    fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "drk" | "darkfi" => Ok(NetworkName::DarkFi),
            "sol" | "solana" => Ok(NetworkName::Solana),
            "btc" | "bitcoin" => Ok(NetworkName::Bitcoin),
            "eth" | "ethereum" => Ok(NetworkName::Ethereum),
            _ => Err(crate::Error::UnsupportedCoinNetwork),
        }
    }
}

impl Encodable for NetworkName {
    fn encode<S: io::Write>(&self, s: S) -> core::result::Result<usize, io::Error> {
        let name = self.to_string();
        let len = name.encode(s)?;
        Ok(len)
    }
}

impl Decodable for NetworkName {
    fn decode<D: io::Read>(mut d: D) -> core::result::Result<Self, io::Error> {
        let name: String = Decodable::decode(&mut d)?;
        match NetworkName::from_str(&name) {
            Ok(v) => Ok(v),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e)),
        }
    }
}
