/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 * Copyright (C) 2021 The Tari Project (BSD-3)
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

use monero::{
    consensus::Encodable,
    cryptonote::hash::Hashable,
    util::ringct::{RctSigBase, RctType},
    Hash,
};
use tiny_keccak::Hasher;

use crate::blockchain::monero::MoneroPowData;

mod helpers;
use helpers::create_blockhashing_blob;

pub mod merkle_tree_parameters;

impl MoneroPowData {
    /// Returns true if the coinbase Merkle proof produces the `merkle_root` hash.
    pub fn is_coinbase_valid_merkle_root(&self) -> bool {
        let mut finalised_prefix_keccak = self.coinbase_tx_hasher.clone();
        let mut encoder_extra_field = vec![];
        self.coinbase_tx_extra.consensus_encode(&mut encoder_extra_field).unwrap();
        finalised_prefix_keccak.update(&encoder_extra_field);
        let mut prefix_hash: [u8; 32] = [0u8; 32];
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

    /// Returns the blockhashing_blob for the Monero block
    pub fn to_blockhashing_blob(&self) -> Vec<u8> {
        create_blockhashing_blob(&self.header, &self.merkle_root, u64::from(self.transaction_count))
    }

    /// Returns the RandomX VM key
    pub fn randomx_key(&self) -> &[u8] {
        self.randomx_key.as_slice()
    }
}
