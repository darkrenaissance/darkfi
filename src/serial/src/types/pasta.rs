/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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
use std::io::{Error, ErrorKind, Read, Result, Write};

#[cfg(feature = "async")]
use crate::{
    async_lib::{AsyncReadExt, AsyncWriteExt},
    AsyncDecodable, AsyncEncodable,
};
#[cfg(feature = "async")]
use async_trait::async_trait;
#[cfg(feature = "async")]
use futures_lite::{AsyncRead, AsyncWrite};

use pasta_curves::{
    group::{ff::PrimeField, GroupEncoding},
    Ep, Eq, Fp, Fq,
};

use crate::{Decodable, Encodable, ReadExt, WriteExt};

impl Encodable for Fp {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
        s.write_slice(&self.to_repr())?;
        Ok(32)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncEncodable for Fp {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        s.write_slice_async(&self.to_repr()).await?;
        Ok(32)
    }
}

impl Decodable for Fp {
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        match Self::from_repr(bytes).into() {
            Some(v) => Ok(v),
            None => Err(Error::new(ErrorKind::Other, "Noncanonical bytes for pallas::Base")),
        }
    }
}
#[cfg(feature = "async")]
#[async_trait]
impl AsyncDecodable for Fp {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let mut bytes = [0u8; 32];
        d.read_slice_async(&mut bytes).await?;
        match Self::from_repr(bytes).into() {
            Some(v) => Ok(v),
            None => Err(Error::new(ErrorKind::Other, "Noncanonical bytes for pallas::Base")),
        }
    }
}

impl Encodable for Fq {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
        s.write_slice(&self.to_repr())?;
        Ok(32)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncEncodable for Fq {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        s.write_slice_async(&self.to_repr()).await?;
        Ok(32)
    }
}

impl Decodable for Fq {
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        match Self::from_repr(bytes).into() {
            Some(v) => Ok(v),
            None => Err(Error::new(ErrorKind::Other, "Noncanonical bytes for pallas::Scalar")),
        }
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncDecodable for Fq {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let mut bytes = [0u8; 32];
        d.read_slice_async(&mut bytes).await?;
        match Self::from_repr(bytes).into() {
            Some(v) => Ok(v),
            None => Err(Error::new(ErrorKind::Other, "Noncanonical bytes for pallas::Scalar")),
        }
    }
}

impl Encodable for Ep {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
        s.write_slice(&self.to_bytes())?;
        Ok(32)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncEncodable for Ep {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        s.write_slice_async(&self.to_bytes()).await?;
        Ok(32)
    }
}

impl Decodable for Ep {
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        match Self::from_bytes(&bytes).into() {
            Some(v) => Ok(v),
            None => Err(Error::new(ErrorKind::Other, "Noncanonical bytes for pallas::Point")),
        }
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncDecodable for Ep {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let mut bytes = [0u8; 32];
        d.read_slice_async(&mut bytes).await?;
        match Self::from_bytes(&bytes).into() {
            Some(v) => Ok(v),
            None => Err(Error::new(ErrorKind::Other, "Noncanonical bytes for pallas::Point")),
        }
    }
}

impl Encodable for Eq {
    fn encode<S: Write>(&self, s: &mut S) -> Result<usize> {
        s.write_slice(&self.to_bytes())?;
        Ok(32)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncEncodable for Eq {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        s.write_slice_async(&self.to_bytes()).await?;
        Ok(32)
    }
}

impl Decodable for Eq {
    fn decode<D: Read>(d: &mut D) -> Result<Self> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        match Self::from_bytes(&bytes).into() {
            Some(v) => Ok(v),
            None => Err(Error::new(ErrorKind::Other, "Noncanonical bytes for vesta::Point")),
        }
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncDecodable for Eq {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let mut bytes = [0u8; 32];
        d.read_slice_async(&mut bytes).await?;
        match Self::from_bytes(&bytes).into() {
            Some(v) => Ok(v),
            None => Err(Error::new(ErrorKind::Other, "Noncanonical bytes for vesta::Point")),
        }
    }
}
