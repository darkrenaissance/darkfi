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

use darkfi::consensus::lead_coin::LeadCoin;
use darkfi_serial::serialize;

/// Extract currently configured consensus headstart parameter
/// pallas::Base representation and print its bs58 encoding.
fn main() {
    let headstart = bs58::encode(&serialize(&LeadCoin::headstart())).into_string();
    println!("Currently configured headstart(encoded): {headstart}");
}

#[cfg(test)]
mod tests {
    use darkfi::{consensus::lead_coin::LeadCoin, Error, Result};
    use darkfi_sdk::{crypto::pasta_prelude::PrimeField, pasta::pallas};
    use darkfi_serial::serialize;

    #[test]
    fn test_conversion() -> Result<()> {
        let headstart = LeadCoin::headstart();
        let encoded = bs58::encode(&serialize(&headstart)).into_string();
        let pallas = from_bs58str(&encoded)?;
        assert_eq!(headstart, pallas);

        Ok(())
    }

    fn from_bs58str(s: &str) -> Result<pallas::Base> {
        let decoded = bs58::decode(s).into_vec()?;
        if decoded.len() != 32 {
            return Err(Error::DecodeError("Decoded bs58 string is not 32 bytes long"))
        }
        let bytes: [u8; 32] = decoded.try_into().unwrap();
        Ok(pallas::Base::from_repr(bytes).unwrap())
    }
}
