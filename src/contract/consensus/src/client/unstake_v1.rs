/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

//! This API is crufty. Please rework it into something nice to read and nice to use.

use darkfi::{
    zk::{Proof, ProvingKey},
    zkas::ZkBinary,
    Result,
};
use darkfi_money_contract::model::{ConsensusUnstakeParamsV1, UnstakeInput};
use darkfi_sdk::{
    crypto::{pasta_prelude::*, MerkleTree, SecretKey},
    incrementalmerkletree::Tree,
    pasta::pallas,
};
use log::{debug, info};
use rand::rngs::OsRng;

use crate::client::{
    common::{create_consensus_burn_proof, TransactionBuilderConsensusInputInfo},
    ConsensusOwnCoin,
};

pub struct ConsensusUnstakeCallDebris {
    pub params: ConsensusUnstakeParamsV1,
    pub proofs: Vec<Proof>,
    pub signature_secret: SecretKey,
    pub value_blind: pallas::Scalar,
}

/// Struct holding necessary information to build a `Consensus::UnstakeV1` contract call.
pub struct ConsensusUnstakeCallBuilder {
    /// `ConsensusOwnCoin` we're given to use in this builder
    pub coin: ConsensusOwnCoin,
    /// Merkle tree of coins used to create inclusion proofs
    pub tree: MerkleTree,
    /// `ConsensusBurn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for the `ConsensusBurn_V1` zk circuit
    pub burn_pk: ProvingKey,
}

impl ConsensusUnstakeCallBuilder {
    pub fn build(&self) -> Result<ConsensusUnstakeCallDebris> {
        debug!("Building Consensus::UnstakeV1 contract call");
        assert!(self.coin.note.value != 0);

        debug!("Building anonymous input");
        let leaf_position = self.coin.leaf_position;
        let root = self.tree.root(0).unwrap();
        let merkle_path = self.tree.authentication_path(leaf_position, &root).unwrap();
        let input = TransactionBuilderConsensusInputInfo {
            leaf_position,
            merkle_path,
            secret: self.coin.secret,
            note: self.coin.note.clone(),
        };
        debug!("Finished building input");

        let value_blind = pallas::Scalar::random(&mut OsRng);
        info!("Creating unstake burn proof for input");
        let (proof, public_inputs, signature_secret) =
            create_consensus_burn_proof(&self.burn_zkbin, &self.burn_pk, &input, value_blind)?;

        let input = UnstakeInput {
            epoch: self.coin.note.epoch,
            value_commit: public_inputs.value_commit,
            nullifier: public_inputs.nullifier,
            merkle_root: public_inputs.merkle_root,
            signature_public: public_inputs.signature_public,
        };

        // We now fill this with necessary stuff
        let params = ConsensusUnstakeParamsV1 { input };
        let proofs = vec![proof];

        // Now we should have all the params, zk proof, signature secret and token blind.
        // We return it all and let the caller deal with it.
        let debris = ConsensusUnstakeCallDebris { params, proofs, signature_secret, value_blind };
        Ok(debris)
    }
}
