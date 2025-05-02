use monero::Hash;

use crate::error::MergeMineError;

const MAX_MERKLE_TREE_PROOF_SIZE: usize = 32;

/// Returns the Keccak 256 hash of the byte input
fn cn_fast_hash(data: &[u8]) -> Hash {
    Hash::new(data)
}

/// Returns the Keccak 256 hash of 2 hashes
fn cn_fast_hash2(hash1: &Hash, hash2: &Hash) -> Hash {
    let mut tmp = [0u8; 64];
    tmp[..32].copy_from_slice(hash1.as_bytes());
    tmp[32..].copy_from_slice(hash2.as_bytes());
    cn_fast_hash(&tmp)
}

/// Round down to power of two.
/// Should error for count < 3 or if the count is unreasonably large
/// for tree hash calculations.
fn tree_hash_count(count: usize) -> Result<usize, MergeMineError> {
    if count < 3 {
        return Err(MergeMineError::HashingError(format!(
            "Cannot calculate tree hash root. Expected count to be >3 but got {}",
            count
        )));
    }

    if count > 0x10000000 {
        return Err(MergeMineError::HashingError(format!(
            "Cannot calculate tree hash root. Expected count to be less than 0x10000000 but got {}",
            count
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
pub fn tree_hash(hashes: &[Hash]) -> Result<Hash, MergeMineError> {
    if hashes.is_empty() {
        return Err(MergeMineError::HashingError(
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
                return Err(MergeMineError::HashingError(
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

/// The Monero Merkle proof
#[derive(Debug, Clone)]
pub struct MerkleProof {
    branch: Vec<Hash>,
    path_bitmap: u32,
}

impl MerkleProof {
    fn try_construct(branch: Vec<Hash>, path_bitmap: u32) -> Option<Self> {
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

/// Creates a Merkle proof for the given hash within the set of hashes.
/// This function returns None if the hash is not in hashes.
/// This is a port of Monero's tree_branch function.
#[allow(clippy::cognitive_complexity)]
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
