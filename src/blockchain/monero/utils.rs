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

use monero::{Hash, VarInt};
use primitive_types::U256;
use sha2::{Digest, Sha256};

use super::{MerkleProof, MerkleTreeParameters, MoneroPowData};
use crate::{blockchain::HeaderHash, Error, Result};

/// Returns the Keccak 256 hash of the byte input
pub fn cn_fast_hash(data: &[u8]) -> Hash {
    Hash::new(data)
}

/// Returns the Keccak 256 hash of 2 hashes
pub fn cn_fast_hash2(hash1: &Hash, hash2: &Hash) -> Hash {
    let mut tmp = [0u8; 64];
    tmp[..32].copy_from_slice(hash1.as_bytes());
    tmp[32..].copy_from_slice(hash2.as_bytes());
    cn_fast_hash(&tmp)
}

/// Round down to power of two.
/// Should error for count < 3 or if the count is unreasonably large
/// for tree hash calculations.
#[allow(unused)]
fn tree_hash_count(count: usize) -> Result<usize> {
    if count < 3 {
        return Err(Error::MoneroHashingError(format!(
            "Cannot calculate tree hash root. Expected count to be >3 but got {count}"
        )));
    }

    if count > 0x10000000 {
        return Err(Error::MoneroHashingError(format!(
            "Cannot calculate tree hash root. Expected count to be less than 0x10000000 but got {count}"
        )));
    }

    // Essentially we are doing 1 << floor(log2(count))
    let mut pow: usize = 2;
    while pow < count {
        pow <<= 1;
    }

    Ok(pow >> 1)
}

/// Tree hash algorithm in Monero
#[allow(unused)]
pub fn tree_hash(hashes: &[Hash]) -> Result<Hash> {
    if hashes.is_empty() {
        return Err(Error::MoneroHashingError(
            "Cannot calculate Merkle root, no hashes".to_string(),
        ));
    }

    match hashes.len() {
        1 => Ok(hashes[0]),
        2 => Ok(cn_fast_hash2(&hashes[0], &hashes[1])),
        n => {
            let mut cnt = tree_hash_count(n)?;
            let mut buf = vec![Hash::null(); cnt];

            // c is the number of elements between the number of hashes
            // and the next power of 2.
            let c = 2 * cnt - hashes.len();

            buf[..c].copy_from_slice(&hashes[..c]);

            // hash the rest of the hashes together
            let mut i: usize = c;
            for b in &mut buf[c..cnt] {
                *b = cn_fast_hash2(&hashes[i], &hashes[i + 1]);
                i += 2;
            }

            if i != hashes.len() {
                return Err(Error::MoneroHashingError(
                    "Cannot calculate the Merkle root, hashes not equal to count".to_string(),
                ));
            }

            while cnt > 2 {
                cnt >>= 1;
                let mut i = 0;
                for j in 0..cnt {
                    buf[j] = cn_fast_hash2(&buf[i], &buf[i + 1]);
                    i += 2;
                }
            }

            Ok(cn_fast_hash2(&buf[0], &buf[1]))
        }
    }
}

/// Creates a Merkle proof for the given hash within the set of hashes.
/// This function returns None if the hash is not in hashes.
/// This is a port of Monero's tree_branch function.
#[allow(clippy::cognitive_complexity)]
#[allow(unused)]
pub fn create_merkle_proof(hashes: &[Hash], hash: &Hash) -> Option<MerkleProof> {
    match hashes.len() {
        0 => None,
        1 => {
            if hashes[0] != *hash {
                return None;
            }
            MerkleProof::try_construct(vec![], 0)
        }
        2 => hashes.iter().enumerate().find_map(|(pos, h)| {
            if h != hash {
                return None
            }
            let i = usize::from(pos == 0);
            MerkleProof::try_construct(vec![hashes[i]], u32::from(pos != 0))
        }),
        len => {
            let mut idx = hashes.iter().position(|node| node == hash)?;
            let mut count = tree_hash_count(len).ok()?;

            let mut ints = vec![Hash::null(); count];

            let c = 2 * count - len;
            ints[..c].copy_from_slice(&hashes[..c]);

            let mut branch = vec![];
            let mut path = 0u32;
            let mut i = c;
            for (j, val) in ints.iter_mut().enumerate().take(count).skip(c) {
                // Left or right
                if idx == i || idx == i + 1 {
                    let ii = if idx == i { i + 1 } else { i };
                    branch.push(hashes[ii]);
                    path = (path << 1) | u32::from(idx != i);
                    idx = j;
                }
                *val = cn_fast_hash2(&hashes[i], &hashes[i + 1]);
                i += 2;
            }

            debug_assert_eq!(i, len);

            while count > 2 {
                count >>= 1;
                let mut i = 0;
                for j in 0..count {
                    if idx == i || idx == i + 1 {
                        let ii = if idx == i { i + 1 } else { i };
                        branch.push(ints[ii]);
                        path = (path << 1) | u32::from(idx != i);
                        idx = j;
                    }
                    ints[j] = cn_fast_hash2(&ints[i], &ints[i + 1]);
                    i += 2;
                }
            }

            if idx == 0 || idx == 1 {
                let ii = usize::from(idx == 0);
                branch.push(ints[ii]);
                path = (path << 1) | u32::from(idx != 0);
            }

            MerkleProof::try_construct(branch, path)
        }
    }
}

/// Creates a hex-encoded Monero blockhashing_blob
pub fn create_blockhashing_blob(
    header: &monero::BlockHeader,
    merkle_root: &monero::Hash,
    transaction_count: u64,
) -> Vec<u8> {
    let mut blockhashing_blob = monero::consensus::serialize(header);
    blockhashing_blob.extend_from_slice(merkle_root.as_bytes());
    let mut count = monero::consensus::serialize(&VarInt(transaction_count));
    blockhashing_blob.append(&mut count);
    blockhashing_blob
}

#[allow(unused)]
fn check_aux_chains(
    monero_data: &MoneroPowData,
    merge_mining_params: VarInt,
    aux_chain_merkle_root: &monero::Hash,
    darkfi_hash: HeaderHash,
    darkfi_genesis_hash: HeaderHash,
) -> bool {
    let df_hash = monero::Hash::from_slice(darkfi_hash.as_slice());

    if merge_mining_params == VarInt(0) {
        // Interpret 0 as only 1 chain
        if df_hash == *aux_chain_merkle_root {
            return true
        }
    }

    let merkle_tree_params = MerkleTreeParameters::from_varint(merge_mining_params);
    if merkle_tree_params.number_of_chains() == 0 {
        return false
    }

    let hash_position = U256::from_little_endian(
        &Sha256::new()
            .chain_update(darkfi_genesis_hash.as_slice())
            .chain_update(merkle_tree_params.aux_nonce().to_le_bytes())
            .chain_update((109_u8).to_le_bytes())
            .finalize(),
    )
    .low_u32() %
        u32::from(merkle_tree_params.number_of_chains());

    let (merkle_root, pos) = monero_data
        .aux_chain_merkle_proof
        .calculate_root_with_pos(&df_hash, merkle_tree_params.number_of_chains());

    if hash_position != pos {
        return false
    }

    merkle_root == *aux_chain_merkle_root
}
