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
use darkfi_serial::{AsyncDecodable, AsyncEncodable, AsyncRead, AsyncWrite, Decodable, Encodable};
use monero::{
    blockdata::transaction::RawExtraField,
    consensus::{Decodable as XmrDecodable, Encodable as XmrEncodable},
    cryptonote::hash::Hashable,
    util::ringct::{RctSigBase, RctType},
    BlockHeader, Hash,
};
use tiny_keccak::{Hasher, Keccak};

mod merkle_proof;
use merkle_proof::MerkleProof;

mod keccak;
use keccak::{keccak_from_bytes, keccak_to_bytes};

mod utils;

/// This struct represents all the Proof of Work information required
/// for merge mining.
#[derive(Clone)]
pub struct MoneroPowData {
    /// Monero Header fields
    pub header: BlockHeader,
    /// RandomX VM key - length varies to a max len of 60.
    /// TODO: Implement a type, or use randomx_key[0] to define len.
    pub randomx_key: [u8; 64],
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

#[async_trait]
impl Decodable for MoneroPowData {
    fn decode<D: Read>(d: &mut D) -> io::Result<Self> {
        let header =
            BlockHeader::consensus_decode(d).map_err(|_| Error::other("Invalid XMR header"))?;

        let randomx_key: [u8; 64] = Decodable::decode(d)?;
        let transaction_count: u16 = Decodable::decode(d)?;

        let merkle_root =
            Hash::consensus_decode(d).map_err(|_| Error::other("Invamid XMR hash"))?;

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

#[async_trait]
impl AsyncDecodable for MoneroPowData {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> io::Result<Self> {
        let buf: Vec<u8> = AsyncDecodable::decode_async(d).await?;
        let mut buf = Cursor::new(buf);
        let header = BlockHeader::consensus_decode(&mut buf)
            .map_err(|_| Error::other("Invalid XMR header"))?;

        let randomx_key: [u8; 64] = AsyncDecodable::decode_async(d).await?;
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

impl MoneroPowData {
    /// Returns true if the coinbase Merkle proof produces the `merkle_root` hash.
    pub fn is_coinbase_valid_merkle_root(&self) -> bool {
        let mut finalised_prefix_keccak = self.coinbase_tx_hasher.clone();
        let mut encoder_extra_field = vec![];
        self.coinbase_tx_extra.consensus_encode(&mut encoder_extra_field).unwrap();
        finalised_prefix_keccak.update(&encoder_extra_field);
        let mut prefix_hash: [u8; 32] = [0; 32];
        finalised_prefix_keccak.finalize(&mut prefix_hash);

        let final_prefix_hash = Hash::from_slice(&prefix_hash);

        // let mut finalised_keccak = Keccak::v256();
        let rct_sig_base = RctSigBase {
            rct_type: RctType::Null,
            txn_fee: Default::default(),
            pseudo_outs: vec![],
            ecdh_info: vec![],
            out_pk: vec![],
        };

        let hashes = vec![final_prefix_hash, rct_sig_base.hash(), Hash::null()];
        let encoder_final: Vec<u8> =
            hashes.into_iter().flat_map(|h| Vec::from(&h.to_bytes()[..])).collect();
        let coinbase_hash = Hash::new(encoder_final);

        let merkle_root = self.coinbase_merkle_proof.calculate_root(&coinbase_hash);
        (self.merkle_root == merkle_root) && self.coinbase_merkle_proof.check_coinbase_path()
    }
}
