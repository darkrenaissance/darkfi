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

use std::{
    io::{self, Read, Write},
    ops::Deref,
};

#[cfg(feature = "async-serial")]
use darkfi_serial::{
    async_trait, AsyncDecodable, AsyncEncodable, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt,
};
use darkfi_serial::{Decodable, Encodable, ReadExt, WriteExt};

const MAX_ARR_SIZE: usize = 60;

/// A fixed-size byte array for RandomX that can be serialized and deserialized.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixedByteArray {
    elems: [u8; MAX_ARR_SIZE],
    len: u8,
}

impl FixedByteArray {
    /// Create a new FixedByteArray with the preset length.
    /// The array will be zeroed.
    pub fn new() -> Self {
        Default::default()
    }

    /// Returns the array as a slice of bytes.
    pub fn as_slice(&self) -> &[u8] {
        &self[..self.len()]
    }

    /// Returns true if the array is full.
    #[inline]
    pub fn is_full(&self) -> bool {
        self.len() == MAX_ARR_SIZE
    }

    /// Returns the length of the array.
    #[inline]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Returns true if the array is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn to_vec(&self) -> Vec<u8> {
        self.as_slice().to_vec()
    }
}

impl Deref for FixedByteArray {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.elems[..self.len as usize]
    }
}

impl Default for FixedByteArray {
    fn default() -> Self {
        Self { elems: [0u8; MAX_ARR_SIZE], len: 0 }
    }
}

impl Encodable for FixedByteArray {
    fn encode<S: Write>(&self, s: &mut S) -> io::Result<usize> {
        let mut n = 1;
        s.write_u8(self.len)?;
        let data = self.as_slice();
        for item in data.iter().take(self.len as usize) {
            s.write_u8(*item)?;
            n += 1;
        }

        Ok(n)
    }
}

#[cfg(feature = "async-serial")]
#[async_trait]
impl AsyncEncodable for FixedByteArray {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> io::Result<usize> {
        let mut n = 1;
        s.write_u8_async(self.len).await?;
        let data = self.as_slice();
        for item in data.iter().take(self.len as usize) {
            s.write_u8_async(*item).await?;
            n += 1;
        }

        Ok(n)
    }
}

impl Decodable for FixedByteArray {
    fn decode<D: Read>(d: &mut D) -> io::Result<Self> {
        let len = d.read_u8()? as usize;
        if len > MAX_ARR_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("length exceeded max of 60 bytes for FixedByteArray: {}", len),
            ));
        }

        let mut elems = [0u8; MAX_ARR_SIZE];
        #[allow(clippy::needless_range_loop)]
        for i in 0..len {
            elems[i] = d.read_u8()?;
        }

        Ok(Self { elems, len: len as u8 })
    }
}

#[cfg(feature = "async-serial")]
#[async_trait]
impl AsyncDecodable for FixedByteArray {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> io::Result<Self> {
        let len = d.read_u8_async().await? as usize;
        if len > MAX_ARR_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("length exceeded max of 60 bytes for FixedByteArray: {}", len),
            ));
        }

        let mut elems = [0u8; MAX_ARR_SIZE];
        #[allow(clippy::needless_range_loop)]
        for i in 0..len {
            elems[i] = d.read_u8_async().await?;
        }

        Ok(Self { elems, len: len as u8 })
    }
}
