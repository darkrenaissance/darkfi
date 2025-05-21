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
    fmt,
    io::{self, Cursor, Error, Read, Write},
};

#[cfg(feature = "async-serial")]
use darkfi_serial::{async_trait, AsyncDecodable, AsyncEncodable, AsyncRead, AsyncWrite};
use darkfi_serial::{Decodable, Encodable};
use monero::{
    blockdata::transaction::RawExtraField,
    consensus::{Decodable as XmrDecodable, Encodable as XmrEncodable},
    BlockHeader, Hash,
};
use tiny_keccak::{Hasher, Keccak};

pub mod fixed_array;
use fixed_array::FixedByteArray;

pub mod merkle_proof;
use merkle_proof::MerkleProof;

pub mod keccak;
use keccak::{keccak_from_bytes, keccak_to_bytes};

pub mod utils;

/// This struct represents all the Proof of Work information required
/// for merge mining.
#[derive(Clone)]
pub struct MoneroPowData {
    /// Monero Header fields
    pub header: BlockHeader,
    /// RandomX VM key - length varies to a max len of 60.
    /// TODO: Implement a type, or use randomx_key[0] to define len.
    pub randomx_key: FixedByteArray,
    /// The number of transactions included in this Monero block.
    /// This is used to produce the blockhashing_blob.
    pub transaction_count: u16,
    /// Transaction root
    pub merkle_root: Hash,
    /// Coinbase Merkle proof hashes
    pub coinbase_merkle_proof: MerkleProof,
    /// Incomplete hashed state of the coinbase transaction
    pub coinbase_tx_hasher: Keccak,
    /// Extra field of the coinbase
    pub coinbase_tx_extra: RawExtraField,
    /// Aux chain Merkle proof hashes
    pub aux_chain_merkle_proof: MerkleProof,
}

impl fmt::Debug for MoneroPowData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut digest = [0u8; 32];
        self.coinbase_tx_hasher.clone().finalize(&mut digest);
        f.debug_struct("MoneroPowData")
            .field("header", &self.header)
            .field("randomx_key", &self.randomx_key)
            .field("transaction_count", &self.transaction_count)
            .field("merkle_root", &self.merkle_root)
            .field("coinbase_merkle_proof", &self.coinbase_merkle_proof)
            .field("coinbase_tx_extra", &self.coinbase_tx_extra)
            .field("aux_chain_merkle_proof", &self.aux_chain_merkle_proof)
            .finish()
    }
}

impl Encodable for MoneroPowData {
    fn encode<S: Write>(&self, s: &mut S) -> io::Result<usize> {
        let mut n = 0;

        n += self.header.consensus_encode(s)?;
        n += self.randomx_key.encode(s)?;
        n += self.transaction_count.encode(s)?;
        n += self.merkle_root.consensus_encode(s)?;
        n += self.coinbase_merkle_proof.encode(s)?;

        // This is an incomplete hasher. Dump it from memory
        // and write it down. We can restore it the same way.
        let buf = keccak_to_bytes(&self.coinbase_tx_hasher);
        n += buf.encode(s)?;

        n += self.coinbase_tx_extra.0.encode(s)?;
        n += self.aux_chain_merkle_proof.encode(s)?;

        Ok(n)
    }
}

#[cfg(feature = "async-serial")]
#[async_trait]
impl AsyncEncodable for MoneroPowData {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> io::Result<usize> {
        let mut n = 0;

        let mut buf = vec![];
        self.header.consensus_encode(&mut buf)?;
        n += buf.encode_async(s).await?;

        n += self.randomx_key.encode_async(s).await?;
        n += self.transaction_count.encode_async(s).await?;

        let mut buf = vec![];
        self.merkle_root.consensus_encode(&mut buf)?;
        n += buf.encode_async(s).await?;

        n += self.coinbase_merkle_proof.encode_async(s).await?;

        // This is an incomplete hasher. Dump it from memory
        // and write it down. We can restore it the same way.
        let buf = keccak_to_bytes(&self.coinbase_tx_hasher);
        n += buf.encode_async(s).await?;

        n += self.coinbase_tx_extra.0.encode_async(s).await?;
        n += self.aux_chain_merkle_proof.encode_async(s).await?;

        Ok(n)
    }
}

impl Decodable for MoneroPowData {
    fn decode<D: Read>(d: &mut D) -> io::Result<Self> {
        let header =
            BlockHeader::consensus_decode(d).map_err(|_| Error::other("Invalid XMR header"))?;

        let randomx_key: FixedByteArray = Decodable::decode(d)?;
        let transaction_count: u16 = Decodable::decode(d)?;

        let merkle_root =
            Hash::consensus_decode(d).map_err(|_| Error::other("Invalid XMR hash"))?;

        let coinbase_merkle_proof: MerkleProof = Decodable::decode(d)?;

        let buf: Vec<u8> = Decodable::decode(d)?;
        let coinbase_tx_hasher = keccak_from_bytes(&buf);

        let coinbase_tx_extra: Vec<u8> = Decodable::decode(d)?;
        let coinbase_tx_extra = RawExtraField(coinbase_tx_extra);
        let aux_chain_merkle_proof: MerkleProof = Decodable::decode(d)?;

        Ok(Self {
            header,
            randomx_key,
            transaction_count,
            merkle_root,
            coinbase_merkle_proof,
            coinbase_tx_hasher,
            coinbase_tx_extra,
            aux_chain_merkle_proof,
        })
    }
}

#[cfg(feature = "async-serial")]
#[async_trait]
impl AsyncDecodable for MoneroPowData {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> io::Result<Self> {
        let buf: Vec<u8> = AsyncDecodable::decode_async(d).await?;
        let mut buf = Cursor::new(buf);
        let header = BlockHeader::consensus_decode(&mut buf)
            .map_err(|_| Error::other("Invalid XMR header"))?;

        let randomx_key: FixedByteArray = AsyncDecodable::decode_async(d).await?;
        let transaction_count: u16 = AsyncDecodable::decode_async(d).await?;

        let buf: Vec<u8> = AsyncDecodable::decode_async(d).await?;
        let mut buf = Cursor::new(buf);
        let merkle_root =
            Hash::consensus_decode(&mut buf).map_err(|_| Error::other("Invalid XMR hash"))?;

        let coinbase_merkle_proof: MerkleProof = AsyncDecodable::decode_async(d).await?;

        let buf: Vec<u8> = AsyncDecodable::decode_async(d).await?;
        let coinbase_tx_hasher = keccak_from_bytes(&buf);

        let coinbase_tx_extra: Vec<u8> = AsyncDecodable::decode_async(d).await?;
        let coinbase_tx_extra = RawExtraField(coinbase_tx_extra);
        let aux_chain_merkle_proof: MerkleProof = AsyncDecodable::decode_async(d).await?;

        Ok(Self {
            header,
            randomx_key,
            transaction_count,
            merkle_root,
            coinbase_merkle_proof,
            coinbase_tx_hasher,
            coinbase_tx_extra,
            aux_chain_merkle_proof,
        })
    }
}
