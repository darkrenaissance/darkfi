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

use crate::{Decodable, Encodable};

impl Encodable for semver::Prerelease {
    #[inline]
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        self.as_str().encode(&mut s)
    }
}

impl Decodable for semver::Prerelease {
    #[inline]
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let s: String = Decodable::decode(&mut d)?;

        match Self::new(&s) {
            Ok(v) => Ok(v),
            Err(_e) => Err(Error::new(ErrorKind::Other, "Failed parsing semver Prerelase")),
        }
    }
}

impl Encodable for semver::BuildMetadata {
    #[inline]
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        self.as_str().encode(&mut s)
    }
}

impl Decodable for semver::BuildMetadata {
    #[inline]
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let s: String = Decodable::decode(&mut d)?;

        match Self::new(&s) {
            Ok(v) => Ok(v),
            Err(_e) => Err(Error::new(ErrorKind::Other, "Failed parsing semver BuildMetadata")),
        }
    }
}

impl Encodable for semver::Version {
    #[inline]
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        let mut len = 0;
        len += self.major.encode(&mut s)?;
        len += self.minor.encode(&mut s)?;
        len += self.patch.encode(&mut s)?;
        len += self.pre.encode(&mut s)?;
        len += self.build.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for semver::Version {
    #[inline]
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let major: u64 = Decodable::decode(&mut d)?;
        let minor: u64 = Decodable::decode(&mut d)?;
        let patch: u64 = Decodable::decode(&mut d)?;
        let pre: semver::Prerelease = Decodable::decode(&mut d)?;
        let build: semver::BuildMetadata = Decodable::decode(&mut d)?;
        Ok(Self { major, minor, patch, pre, build })
    }
}

#[cfg(test)]
mod tests {
    use crate::{deserialize, serialize};

    #[test]
    fn serialize_deserialize_semver() {
        let versions = vec!["1.12.0", "1.21.3-beta", "2.0.0-rc.1"];

        for version in versions {
            let original = semver::Version::parse(version).unwrap();
            let serialized = serialize(&original);
            let deserialized = deserialize(&serialized).unwrap();
            assert_eq!(original, deserialized);
        }
    }
}
