/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

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
