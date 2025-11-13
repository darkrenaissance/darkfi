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
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

#[cfg(feature = "async-serial")]
use darkfi_serial::{
    async_trait, AsyncDecodable, AsyncEncodable, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt,
};
use darkfi_serial::{Decodable, Encodable, ReadExt, SerialDecodable, SerialEncodable, WriteExt};

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

    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() > MAX_ARR_SIZE {
            return Err(io::Error::new(io::ErrorKind::OutOfMemory, "Slice too large"))
        }

        let len = u8::try_from(bytes.len()).map_err(|_| io::ErrorKind::OutOfMemory)?;

        let mut elems = [0u8; MAX_ARR_SIZE];
        elems
            .get_mut(..len as usize)
            .expect("Cannot fail")
            .copy_from_slice(bytes.get(..len as usize).expect("Cannot fail"));
        Ok(Self { elems, len })
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
                format!("length exceeded max of 60 bytes for FixedByteArray: {len}"),
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
                format!("length exceeded max of 60 bytes for FixedByteArray: {len}"),
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

/// A vector that has a maximum size of `MAX_SIZE`
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, SerialEncodable, SerialDecodable)]
pub struct MaxSizeVec<T, const MAX_SIZE: usize>
where
    T: Send + Sync,
{
    vec: Vec<T>,
    _marker: PhantomData<T>,
}

impl<T, const MAX_SIZE: usize> Default for MaxSizeVec<T, MAX_SIZE>
where
    T: Send + Sync,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const MAX_SIZE: usize> MaxSizeVec<T, MAX_SIZE>
where
    T: Send + Sync,
{
    /// Creates a new `MaxSizeVec` with a capacity of `MAX_SIZE`
    pub fn new() -> Self {
        Self { vec: Vec::new(), _marker: PhantomData }
    }

    /// Creates a new `MaxSizeVec` with the given data.
    /// Returns an error if the data length exceeds `MAX_SIZE`.
    pub fn new_with_data(data: Vec<T>) -> io::Result<Self> {
        if data.len() > MAX_SIZE {
            return Err(io::Error::new(io::ErrorKind::StorageFull, "Size exceeded"))
        }

        Ok(Self { vec: data, _marker: PhantomData })
    }

    /// Creates a `MaxSizeVec` from the given items, truncating if needed
    pub fn from_items_truncate(items: Vec<T>) -> Self {
        let len = std::cmp::min(items.len(), MAX_SIZE);
        Self { vec: items.into_iter().take(len).collect(), _marker: PhantomData }
    }

    /// Consumes `MaxSizeVec` and returns the inner `Vec<T>`
    pub fn into_vec(self) -> Vec<T> {
        self.vec
    }

    /// Returns the maximum size of the `MaxSizeVec`
    pub fn max_size(&self) -> usize {
        MAX_SIZE
    }

    /// Pushes an item to the `MaxSizeVec`
    pub fn push(&mut self, item: T) -> io::Result<()> {
        if self.vec.len() >= MAX_SIZE {
            return Err(io::Error::new(io::ErrorKind::StorageFull, "Size exceeded"))
        }

        self.vec.push(item);
        Ok(())
    }
}

impl<T, const MAX_SIZE: usize> AsRef<[T]> for MaxSizeVec<T, MAX_SIZE>
where
    T: Send + Sync,
{
    fn as_ref(&self) -> &[T] {
        &self.vec
    }
}

impl<T, const MAX_SIZE: usize> Deref for MaxSizeVec<T, MAX_SIZE>
where
    T: Send + Sync,
{
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.vec
    }
}

impl<T, const MAX_SIZE: usize> DerefMut for MaxSizeVec<T, MAX_SIZE>
where
    T: Send + Sync,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.vec
    }
}

impl<T, const MAX_SIZE: usize> Iterator for MaxSizeVec<T, MAX_SIZE>
where
    T: Send + Sync,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.vec.is_empty() {
            None
        } else {
            Some(self.vec.remove(0))
        }
    }
}

impl<T, const MAX_SIZE: usize> FromIterator<T> for MaxSizeVec<T, MAX_SIZE>
where
    T: Send + Sync,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut vec = vec![];
        for item in iter {
            if vec.len() >= MAX_SIZE {
                break
            }
            vec.push(item);
        }

        Self { vec, _marker: PhantomData }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_size() {
        assert_eq!(std::mem::size_of::<FixedByteArray>(), MAX_ARR_SIZE + 1);
    }

    #[test]
    fn capacity_overflow_does_not_panic() {
        let data = &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f];
        let _result = FixedByteArray::decode(&mut data.as_slice()).unwrap_err();
    }

    #[test]
    fn length_check() {
        let mut buf = [u8::try_from(MAX_ARR_SIZE).unwrap(); MAX_ARR_SIZE + 1];
        let fixed_byte_array = FixedByteArray::decode(&mut buf.as_slice()).unwrap();
        assert_eq!(fixed_byte_array.len(), MAX_ARR_SIZE);
        buf[0] += 1;
        FixedByteArray::decode(&mut buf.as_slice()).unwrap_err();
    }
}
