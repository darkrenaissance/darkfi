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

use std::io::{self, Cursor, Error, Read, Write};

use async_trait::async_trait;
use darkfi_serial::{
    AsyncDecodable, AsyncEncodable, AsyncRead, AsyncReadExt, AsyncWrite, Decodable, Encodable,
    ReadExt,
};
use monero::{
    consensus::{Decodable as XmrDecodable, Encodable as XmrEncodable},
    Hash,
};

use super::utils::cn_fast_hash2;

const MAX_MERKLE_TREE_PROOF_SIZE: usize = 32;

/// The Monero Merkle proof
#[derive(Debug, Clone)]
pub struct MerkleProof {
    branch: Vec<Hash>,
    path_bitmap: u32,
}

impl Encodable for MerkleProof {
    fn encode<S: Write>(&self, s: &mut S) -> io::Result<usize> {
        let mut n = 0;

        let len = self.branch.len() as u8;
        n += len.encode(s)?;

        for hash in &self.branch {
            n += (*hash).consensus_encode(s)?;
        }

        n += self.path_bitmap.encode(s)?;

        Ok(n)
    }
}

#[async_trait]
impl AsyncEncodable for MerkleProof {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> io::Result<usize> {
        let mut n = 0;

        let len = self.branch.len() as u8;
        n += len.encode_async(s).await?;

        for hash in &self.branch {
            let mut buf = vec![];
            (*hash).consensus_encode(&mut buf)?;
            n += buf.encode_async(s).await?;
        }

        n += self.path_bitmap.encode_async(s).await?;

        Ok(n)
    }
}

impl Decodable for MerkleProof {
    fn decode<D: Read>(d: &mut D) -> io::Result<Self> {
        let len: u8 = d.read_u8()?;
        let mut branch = Vec::with_capacity(len as usize);

        for _ in 0..len {
            branch.push(Hash::consensus_decode(d).map_err(|_| Error::other("Invalid XMR hash"))?);
        }

        let path_bitmap: u32 = Decodable::decode(d)?;

        Ok(Self { branch, path_bitmap })
    }
}

#[async_trait]
impl AsyncDecodable for MerkleProof {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> io::Result<Self> {
        let len: u8 = d.read_u8_async().await?;
        let mut branch = Vec::with_capacity(len as usize);

        for _ in 0..len {
            let buf: Vec<u8> = AsyncDecodable::decode_async(d).await?;
            let mut buf = Cursor::new(buf);
            branch.push(
                Hash::consensus_decode(&mut buf).map_err(|_| Error::other("Invalid XMR hash"))?,
            );
        }

        let path_bitmap: u32 = AsyncDecodable::decode_async(d).await?;

        Ok(Self { branch, path_bitmap })
    }
}

impl MerkleProof {
    pub fn try_construct(branch: Vec<Hash>, path_bitmap: u32) -> Option<Self> {
        if branch.len() >= MAX_MERKLE_TREE_PROOF_SIZE {
            return None
        }

        Some(Self { branch, path_bitmap })
    }

    /// Returns the Merkle proof branch as a list of Monero hashes
    #[inline]
    pub fn branch(&self) -> &[Hash] {
        &self.branch
    }

    /// Returns the path bitmap of the proof
    pub fn path(&self) -> u32 {
        self.path_bitmap
    }

    /// The coinbase must be the first transaction in the block, so
    /// that you can't have multiple coinbases in a block. That means
    /// the coinbase is always the leftmost branch in the Merkle tree.
    /// This tests that the given proof is for the leftmost branch in
    /// the Merkle tree.
    pub fn check_coinbase_path(&self) -> bool {
        if self.path_bitmap == 0b00000000 {
            return true;
        }
        false
    }

    /// Calculates the Merkle root hash from the provided Monero hash
    pub fn calculate_root_with_pos(&self, hash: &Hash, aux_chain_count: u8) -> (Hash, u32) {
        let root = self.calculate_root(hash);
        let pos = self.get_position_from_path(u32::from(aux_chain_count));
        (root, pos)
    }

    pub fn calculate_root(&self, hash: &Hash) -> Hash {
        if self.branch.is_empty() {
            return *hash;
        }

        let mut root = *hash;
        let depth = self.branch.len();
        for d in 0..depth {
            if (self.path_bitmap >> (depth - d - 1)) & 1 > 0 {
                root = cn_fast_hash2(&self.branch[d], &root);
            } else {
                root = cn_fast_hash2(&root, &self.branch[d]);
            }
        }

        root
    }

    pub fn get_position_from_path(&self, aux_chain_count: u32) -> u32 {
        if aux_chain_count <= 1 {
            return 0
        }

        let mut depth = 0;
        let mut k = 1;

        while k < aux_chain_count {
            depth += 1;
            k <<= 1;
        }

        k -= aux_chain_count;

        let mut pos = 0;
        let mut path = self.path_bitmap;

        for _i in 1..depth {
            pos = (pos << 1) | (path & 1);
            path >>= 1;
        }

        if pos < k {
            return pos
        }

        (((pos - k) << 1) | (path & 1)) + k
    }
}
