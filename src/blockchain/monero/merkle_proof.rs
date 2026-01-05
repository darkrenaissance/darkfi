/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

#[cfg(feature = "async-serial")]
use darkfi_serial::{
    async_trait, AsyncDecodable, AsyncEncodable, AsyncRead, AsyncReadExt, AsyncWrite,
};
use darkfi_serial::{Decodable, Encodable, ReadExt};
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

#[cfg(feature = "async-serial")]
#[async_trait]
impl AsyncEncodable for MerkleProof {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> io::Result<usize> {
        let mut n = 0;

        let len = self.branch.len() as u8;
        n += len.encode_async(s).await?;

        for hash in &self.branch {
            let mut buf = [0u8; 32];
            (*hash).consensus_encode(&mut &mut buf[..])?;
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

#[cfg(feature = "async-serial")]
#[async_trait]
impl AsyncDecodable for MerkleProof {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> io::Result<Self> {
        let len: u8 = d.read_u8_async().await?;
        let mut branch = Vec::with_capacity(len as usize);

        for _ in 0..len {
            let buf: [u8; 32] = AsyncDecodable::decode_async(d).await?;
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

#[cfg(test)]
mod tests {
    use rand::RngCore;
    use std::{iter, str::FromStr};

    use super::{
        super::utils::{create_merkle_proof, tree_hash},
        *,
    };

    #[test]
    fn test_empty_hashset_has_no_proof() {
        assert!(create_merkle_proof(&[], &Hash::null()).is_none());
    }

    #[test]
    fn test_single_hash_is_its_own_proof() {
        let tx_hashes =
            &[Hash::from_str("fa58575f7d1d377709f1621fac98c758860ca6dc5f2262be9ce5fd131c370d1a")
                .unwrap()];
        let proof = create_merkle_proof(&tx_hashes[..], &tx_hashes[0]).unwrap();
        assert_eq!(proof.branch.len(), 0);
        assert_eq!(proof.calculate_root(&tx_hashes[0]), tx_hashes[0]);

        assert!(create_merkle_proof(&tx_hashes[..], &Hash::null()).is_none());
    }

    #[test]
    fn test_two_hash_proof_construction() {
        let tx_hashes = &[
            "d96756959949db23764592fea0bfe88c790e1fd131dabb676948b343aa9ecc24",
            "77d1a87df131c36da4832a7ec382db9b8fe947576a60ec82cc1c66a220f6ee42",
        ]
        .iter()
        .map(|hash| Hash::from_str(hash).unwrap())
        .collect::<Vec<_>>();

        let expected_root = cn_fast_hash2(&tx_hashes[0], &tx_hashes[1]);
        let proof = create_merkle_proof(tx_hashes, &tx_hashes[0]).unwrap();
        assert_eq!(proof.branch()[0], tx_hashes[1]);
        assert_eq!(proof.branch.len(), 1);
        assert_eq!(proof.branch[0], tx_hashes[1]);
        assert_eq!(proof.path_bitmap, 0b00000000);
        assert_eq!(proof.calculate_root(&tx_hashes[0]), expected_root);

        let proof = create_merkle_proof(tx_hashes, &tx_hashes[1]).unwrap();
        assert_eq!(proof.branch()[0], tx_hashes[0]);
        assert_eq!(proof.calculate_root(&tx_hashes[1]), expected_root);

        assert!(create_merkle_proof(tx_hashes, &Hash::null()).is_none());
    }

    #[test]
    fn test_simple_proof_construction() {
        //        { root }
        //      /        \
        //     h01       h2345
        //   /    \     /    \
        //  h0    h1    h23   h45
        //            /  \    /  \
        //          h2    h3 h4   h5

        let hashes = (1..=6).map(|i| Hash::from([i - 1; 32])).collect::<Vec<_>>();
        let h23 = cn_fast_hash2(&hashes[2], &hashes[3]);
        let h45 = cn_fast_hash2(&hashes[4], &hashes[5]);
        let h01 = cn_fast_hash2(&hashes[0], &hashes[1]);
        let h2345 = cn_fast_hash2(&h23, &h45);
        let expected_root = cn_fast_hash2(&h01, &h2345);

        // Proof for h0
        let proof = create_merkle_proof(&hashes, &hashes[0]).unwrap();
        assert_eq!(proof.calculate_root(&hashes[0]), expected_root);
        assert_eq!(proof.branch().len(), 2);
        assert_eq!(proof.branch()[0], hashes[1]);
        assert_eq!(proof.branch()[1], h2345);
        assert_eq!(proof.path_bitmap, 0b00000000);

        // Proof for h2
        let proof = create_merkle_proof(&hashes, &hashes[2]).unwrap();
        assert_eq!(proof.calculate_root(&hashes[2]), expected_root);
        assert_eq!(proof.path_bitmap, 0b00000001);
        let branch = proof.branch();
        assert_eq!(branch[0], hashes[3]);
        assert_eq!(branch[1], h45);
        assert_eq!(branch[2], h01);
        assert_eq!(branch.len(), 3);

        // Proof for h5
        let proof = create_merkle_proof(&hashes, &hashes[5]).unwrap();
        assert_eq!(proof.calculate_root(&hashes[5]), expected_root);
        assert_eq!(proof.branch.len(), 3);
        assert_eq!(proof.path_bitmap, 0b00000111);
        let branch = proof.branch();
        assert_eq!(branch[0], hashes[4]);
        assert_eq!(branch[1], h23);
        assert_eq!(branch[2], h01);
        assert_eq!(branch.len(), 3);

        // Proof for h4
        let proof = create_merkle_proof(&hashes, &hashes[4]).unwrap();
        assert_eq!(proof.calculate_root(&hashes[4]), expected_root);
        assert_eq!(proof.branch.len(), 3);
        assert_eq!(proof.path_bitmap, 0b00000011);
        let branch = proof.branch();
        assert_eq!(branch[0], hashes[5]);
        assert_eq!(branch[1], h23);
        assert_eq!(branch[2], h01);
        assert_eq!(branch.len(), 3);
    }

    #[test]
    fn test_complex_proof_construction() {
        let tx_hashes = &[
            "d96756959949db23764592fea0bfe88c790e1fd131dabb676948b343aa9ecc24",
            "77d1a87df131c36da4832a7ec382db9b8fe947576a60ec82cc1c66a220f6ee42",
            "c723329b1036e4e05313c6ec3bdda3a2e1ab4db17661cad1a6a33512d9b86bcd",
            "5d863b3d275bacd46dbe8a5f3edce86f88cbc01232bd2788b6f44684076ef8a8",
            "16d945de6c96ea7f986b6c70ad373a9203a1ddd1c5d12effc3c69b8648826deb",
            "ccec8f06c5bab1b87bb9af1a3cba94304f87dc037e03b5d2a00406d399316ff7",
            "c8d52ed0712f0725531f8f72da029201b71e9e215884015f7050dde5f33269e7",
            "4360ba7fe3872fa8bbc9655486a02738ee000d0c48bda84a15d4730fea178519",
            "3c8c6b54dcffc75abff89d604ebf1e216bfcb2844b9720ab6040e8e49ae9743c",
            "6dc19de81e509fba200b652fbdde8fe2aeb99bb9b17e0af79d0c682dff194e08",
            "3ef031981bc4e2375eebd034ffda4e9e89936962ad2c94cfcc3e6d4cfa8a2e8c",
            "9e4b865ebe51dcc9cfb09a9b81e354b8f423c59c902d5a866919f053bfbc374e",
            "fa58575f7d1d377709f1621fac98c758860ca6dc5f2262be9ce5fd131c370d1a",
        ]
        .iter()
        .map(|hash| Hash::from_str(hash).unwrap())
        .collect::<Vec<_>>();

        let expected_root = tree_hash(tx_hashes).unwrap();

        let hash =
            Hash::from_str("fa58575f7d1d377709f1621fac98c758860ca6dc5f2262be9ce5fd131c370d1a")
                .unwrap();
        let proof = create_merkle_proof(tx_hashes, &hash).unwrap();

        assert_eq!(proof.path_bitmap, 0b00001111);

        assert_eq!(proof.calculate_root(&hash), expected_root);

        assert!(!proof.branch().contains(&hash));
        assert!(!proof.branch().contains(&expected_root));
    }

    #[test]
    fn test_big_proof_construction() {
        // 65536 txs is beyond what is reasonable to fit in a block
        let mut thread_rng = rand::thread_rng();
        let tx_hashes = iter::repeat_n((), 0x10000)
            .map(|_| {
                let mut buf = [0u8; 32];
                thread_rng.fill_bytes(&mut buf[..]);
                Hash::from_slice(&buf[..])
            })
            .collect::<Vec<_>>();

        let expected_root = tree_hash(&tx_hashes).unwrap();

        let hash = tx_hashes.last().unwrap();
        let proof = create_merkle_proof(&tx_hashes, hash).unwrap();

        assert_eq!(proof.branch.len(), 16);
        assert_eq!(proof.path_bitmap, 0b1111_1111_1111_1111);

        assert_eq!(proof.calculate_root(hash), expected_root);

        assert!(!proof.branch().contains(hash));
        assert!(!proof.branch().contains(&expected_root));
    }

    // Test that both sync and async serialization formats match.
    // We do some hacks because Monero lib doesn't do async.
    #[test]
    fn test_monero_merkleproof_serde() {
        let tx_hashes = &[
            "d96756959949db23764592fea0bfe88c790e1fd131dabb676948b343aa9ecc24",
            "77d1a87df131c36da4832a7ec382db9b8fe947576a60ec82cc1c66a220f6ee42",
        ]
        .iter()
        .map(|hash| Hash::from_str(hash).unwrap())
        .collect::<Vec<_>>();

        let proof = create_merkle_proof(tx_hashes, &tx_hashes[0]).unwrap();

        let local_ex = smol::LocalExecutor::new();

        let ser_sync = darkfi_serial::serialize(&proof);
        let ser_async = smol::future::block_on(
            local_ex.run(async { darkfi_serial::serialize_async(&proof).await }),
        );

        assert_eq!(ser_sync, ser_async);

        let de_sync: MerkleProof = darkfi_serial::deserialize(&ser_async).unwrap();
        let de_async: MerkleProof = smol::future::block_on(
            local_ex.run(async { darkfi_serial::deserialize_async(&ser_sync).await.unwrap() }),
        );

        assert_eq!(de_sync.branch, proof.branch);
        assert_eq!(de_async.branch, proof.branch);
        assert_eq!(de_sync.path_bitmap, proof.path_bitmap);
        assert_eq!(de_async.path_bitmap, proof.path_bitmap);
    }
}
