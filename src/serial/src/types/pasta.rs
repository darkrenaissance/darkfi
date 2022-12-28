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

//! Implementations for pasta curves
use std::io::{Error, ErrorKind, Read, Write};

use pasta_curves::{
    group::{ff::PrimeField, GroupEncoding},
    Ep, Eq, Fp, Fq,
};

use crate::{Decodable, Encodable, ReadExt, WriteExt};

impl Encodable for Fp {
    #[inline]
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        s.write_slice(&self.to_repr())?;
        Ok(32)
    }
}

impl Decodable for Fp {
    #[inline]
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        match Self::from_repr(bytes).into() {
            Some(v) => Ok(v),
            None => Err(Error::new(ErrorKind::Other, "Noncanonical bytes for pallas::Base")),
        }
    }
}

impl Encodable for Fq {
    #[inline]
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        s.write_slice(&self.to_repr())?;
        Ok(32)
    }
}

impl Decodable for Fq {
    #[inline]
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        match Self::from_repr(bytes).into() {
            Some(v) => Ok(v),
            None => Err(Error::new(ErrorKind::Other, "Noncanonical bytes for pallas::Scalar")),
        }
    }
}

impl Encodable for Ep {
    #[inline]
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        s.write_slice(&self.to_bytes())?;
        Ok(32)
    }
}

impl Decodable for Ep {
    #[inline]
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        match Self::from_bytes(&bytes).into() {
            Some(v) => Ok(v),
            None => Err(Error::new(ErrorKind::Other, "Noncanonical bytes for pallas::Point")),
        }
    }
}

impl Encodable for Eq {
    #[inline]
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        s.write_slice(&self.to_bytes())?;
        Ok(32)
    }
}

impl Decodable for Eq {
    #[inline]
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        match Self::from_bytes(&bytes).into() {
            Some(v) => Ok(v),
            None => Err(Error::new(ErrorKind::Other, "Noncanonical bytes for vesta::Point")),
        }
    }
}
