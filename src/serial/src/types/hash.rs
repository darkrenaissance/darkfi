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

use std::io::{Error, Read, Write};

use crate::{Decodable, Encodable, ReadExt, WriteExt};

#[cfg(feature = "blake3")]
impl Encodable for blake3::Hash {
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        s.write_slice(self.as_bytes())?;
        Ok(32)
    }
}

#[cfg(feature = "blake3")]
impl Decodable for blake3::Hash {
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        Ok(bytes.into())
    }
}
