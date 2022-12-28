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

use std::io::{Error, ErrorKind, Read, Write};

use url::Url;

use crate::{Decodable, Encodable};

impl Encodable for Url {
    #[inline]
    fn encode<S: Write>(&self, s: S) -> Result<usize, Error> {
        self.as_str().to_string().encode(s)
    }
}

impl Decodable for Url {
    #[inline]
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let s: String = Decodable::decode(&mut d)?;
        match Url::parse(&s) {
            Ok(v) => Ok(v),
            Err(e) => Err(Error::new(ErrorKind::Other, e)),
        }
    }
}
