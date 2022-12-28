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

use x25519_dalek::PublicKey as X25519PublicKey;

use crate::{Decodable, Encodable, ReadExt, WriteExt};

impl Encodable for X25519PublicKey {
    #[inline]
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        s.write_slice(self.as_bytes())?;
        Ok(32)
    }
}

impl Decodable for X25519PublicKey {
    #[inline]
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        Ok(Self::from(bytes))
    }
}
