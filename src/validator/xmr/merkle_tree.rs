/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
 * Copyright (C) 2021      The Tari Project (BSD-3)
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

use crate::{error::MergeMineError, Result};

/// Binary tree of depth 32 means u32::MAX tree.
/// This is more than large enough to support most trees.
const MAX_MERKLE_TREE_PROOF_SIZE: usize = 32;

/// The Monero Merkle proof
#[derive(Debug, Clone)]
pub struct MerkleProof {
    branch: Vec<monero::Hash>,
    path_bitmap: u32,
}

impl MerkleProof {
    fn try_construct(branch: Vec<monero::Hash>, path_bitmap: u32) -> Option<Self> {
        if branch.len() >= MAX_MERKLE_TREE_PROOF_SIZE {
            return None;
        }

        Some(Self { branch, path_bitmap })
    }
}

/// Returns the Keccak 256 hash of the byte input
fn cn_fast_hash(data: &[u8]) -> monero::Hash {
    monero::Hash::new(data)
}

/// Returns the Keccak 256 hash of 2 hashes
fn cn_fast_hash2(hash1: &monero::Hash, hash2: &monero::Hash) -> monero::Hash {
    let mut tmp = [0u8; 64];
    tmp[..32].copy_from_slice(hash1.as_bytes());
    tmp[32..].copy_from_slice(hash2.as_bytes());
    cn_fast_hash(&tmp)
}

/// Round down to power of 2.
/// Will return an error for count<3 or if the count is unreasonably large for
/// tree hash calculations.
fn tree_hash_count(count: usize) -> Result<usize> {
    if count < 3 {
        return Err(MergeMineError::HashingError(format!(
            "Cannot calculate tree hash root, expected count>3, but got {}",
            count
        ))
        .into())
    }

    if count > 0x10000000 {
        return Err(MergeMineError::HashingError(format!(
            "Cannot calculate tree hash root, expected count<0x10000000, but got {}",
            count
        ))
        .into())
    }

    // Essentially we are doing 1 << floor(log2(count))
    let mut pow: usize = 2;
    while pow < count {
        pow <<= 1;
    }

    Ok(pow >> 1)
}

/// Tree hash algorithm in Monero
pub fn tree_hash(hashes: &[monero::Hash]) -> Result<monero::Hash> {
    if hashes.is_empty() {
        return Err(MergeMineError::HashingError(
            "Cannot calculate Merkle root, `hashes` is empty".to_string(),
        )
        .into())
    }

    match hashes.len() {
        1 => Ok(hashes[0]),
        2 => Ok(cn_fast_hash2(&hashes[0], &hashes[1])),
        n => {
            let mut cnt = tree_hash_count(n)?;
            let mut buf = vec![monero::Hash::null(); cnt];

            // c is the number of elements between the number of hashes
            // and the next power of 2
            let c = 2 * cnt - hashes.len();

            buf[..c].copy_from_slice(&hashes[..c]);

            // Hash the rest of the hashes together
            let mut i: usize = c;
            for b in &mut buf[c..cnt] {
                *b = cn_fast_hash2(&hashes[i], &hashes[i + 1]);
                i += 2;
            }

            if i != hashes.len() {
                return Err(MergeMineError::HashingError(
                    "Cannot calculate Merkle root, hashes not equal to count".to_string(),
                )
                .into());
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
/// This is a port of Monero's `tree_branch` function.
#[allow(clippy::cognitive_complexity)]
pub fn create_merkle_proof(hashes: &[monero::Hash], hash: &monero::Hash) -> Option<MerkleProof> {
    match hashes.len() {
        0 => None,
        1 => {
            if hashes[0] != *hash {
                return None
            }
            MerkleProof::try_construct(vec![], 0)
        }
        2 => hashes.iter().enumerate().find_map(|(pos, h)| {
            if h != hash {
                return None;
            }
            let i = usize::from(pos == 0);
            MerkleProof::try_construct(vec![hashes[i]], u32::from(pos != 0))
        }),
        len => {
            let mut idx = hashes.iter().position(|node| node == hash)?;
            let mut count = tree_hash_count(len).ok()?;

            let mut ints = vec![monero::Hash::null(); count];

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
