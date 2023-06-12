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
use darkfi_money_contract::{
    client::ConsensusOwnCoin,
    model::{ConsensusInput, ConsensusUnstakeParamsV1},
};
use darkfi_sdk::{
    crypto::{pasta_prelude::*, MerkleTree, SecretKey},
    pasta::pallas,
};
use log::{debug, info};
use rand::rngs::OsRng;

use crate::client::common::{create_consensus_burn_proof, ConsensusBurnInputInfo};

pub struct ConsensusUnstakeCallDebris {
    /// Payload params
    pub params: ConsensusUnstakeParamsV1,
    /// ZK proofs
    pub proofs: Vec<Proof>,
    /// Secret key used to sign the transaction
    pub signature_secret: SecretKey,
    /// Value blind to be used in the `Money::UnstakeV1` call
    pub value_blind: pallas::Scalar,
}

/// Struct holding necessary information to build a `Consensus::UnstakeV1` contract call.
pub struct ConsensusUnstakeCallBuilder {
    /// `ConsensusOwnCoin` we're given to use in this builder
    pub owncoin: ConsensusOwnCoin,
    /// Merkle tree of coins used to create inclusion proofs
    pub tree: MerkleTree,
    /// `ConsensusBurn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for the `ConsensusBurn_V1` zk circuit
    pub burn_pk: ProvingKey,
}

impl ConsensusUnstakeCallBuilder {
    pub fn build(&self) -> Result<ConsensusUnstakeCallDebris> {
        info!("Building Consensus::UnstakeV1 contract call");
        assert!(self.owncoin.note.value != 0);

        debug!("Building Consensus::UnstakeV1 anonymous input");
        let root = self.tree.root(0).unwrap();
        let merkle_path = self.tree.authentication_path(self.owncoin.leaf_position, &root).unwrap();

        let input = ConsensusBurnInputInfo {
            leaf_position: self.owncoin.leaf_position,
            merkle_path,
            secret: self.owncoin.secret,
            note: self.owncoin.note.clone(),
            value_blind: pallas::Scalar::random(&mut OsRng),
        };

        info!("Building Consensus::UnstakeV1 Burn ZK proof");
        let (proof, public_inputs, signature_secret) =
            create_consensus_burn_proof(&self.burn_zkbin, &self.burn_pk, &input)?;

        let tx_input = ConsensusInput {
            epoch: self.owncoin.note.epoch,
            value_commit: public_inputs.value_commit,
            nullifier: public_inputs.nullifier,
            merkle_root: public_inputs.merkle_root,
            signature_public: public_inputs.signature_public,
        };

        // We now fill this with necessary stuff
        let params = ConsensusUnstakeParamsV1 { input: tx_input };

        // Construct debris
        let debris = ConsensusUnstakeCallDebris {
            params,
            proofs: vec![proof],
            signature_secret,
            value_blind: input.value_blind,
        };
        Ok(debris)
    }
}
