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

use core::{fmt, str::FromStr};
use std::io;

use darkfi_serial::{SerialDecodable, SerialEncodable};
use pasta_curves::{group::ff::PrimeField, pallas};

/// The `Coin` is represented as a base field element.
#[repr(C)]
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Coin(pub pallas::Base);

impl Coin {
    /// Reference the raw inner base field element
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    /// Try to create a `Coin` type from the given 32 bytes.
    /// Returns `Some` if the bytes fit in the base field, and `None` if not.
    pub fn from_bytes(bytes: [u8; 32]) -> Option<Self> {
        let n = pallas::Base::from_repr(bytes);
        match bool::from(n.is_some()) {
            true => Some(Self(n.unwrap())),
            false => None,
        }
    }

    /// Convert the `Coin` type into 32 raw bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }
}

impl From<pallas::Base> for Coin {
    fn from(x: pallas::Base) -> Self {
        Self(x)
    }
}

impl fmt::Display for Coin {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", bs58::encode(self.to_bytes()).into_string())
    }
}

impl FromStr for Coin {
    type Err = io::Error;

    /// Tries to decode a base58 string into a `Coin` type.
    /// This string is the same string received by calling `Coin::to_string()`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = match bs58::decode(s).into_vec() {
            Ok(v) => v,
            Err(e) => return Err(io::Error::new(io::ErrorKind::Other, e)),
        };

        if bytes.len() != 32 {
            return Err(io::Error::new(io::ErrorKind::Other, "Length of decoded bytes is not 32"))
        }

        if let Some(coin) = Self::from_bytes(bytes.try_into().unwrap()) {
            return Ok(coin)
        }

        Err(io::Error::new(io::ErrorKind::Other, "Invalid bytes for Coin"))
    }
}
