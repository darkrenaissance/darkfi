//pub mod cashier;
pub mod bridge;
pub mod gateway;
pub mod reqrep;

#[cfg(feature = "btc")]
pub mod btc;
#[cfg(feature = "btc")]
pub use btc::{BitcoinKeys, BtcFailed, BtcResult, PubAddress};

#[cfg(feature = "sol")]
pub mod sol;
#[cfg(feature = "sol")]
pub use sol::{SolClient, SolFailed, SolResult};

pub use gateway::{GatewayClient, GatewayService, GatewaySlabsSubscriber};

use crate::serial::{Decodable, Encodable};
use crate::Result;
use std::str::FromStr;

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
        match s {
            "sol" | "Sol" | "solana" | "Solana" | "SOLANA" => Ok(NetworkName::Solana),
            "btc" | "Btc" | "bitcoin" | "Bitcoin" | "BITCOIN" => Ok(NetworkName::Bitcoin),
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
