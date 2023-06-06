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
    client::{ConsensusNote, ConsensusOwnCoin},
    model::{ConsensusInput, ConsensusOutput, ConsensusStakeParamsV1},
};
use darkfi_sdk::{
    crypto::{note::AeadEncryptedNote, pasta_prelude::*, MerkleTree, SecretKey},
    incrementalmerkletree::Tree,
    pasta::pallas,
};
use log::{debug, info};
use rand::rngs::OsRng;

use crate::client::common::{
    create_consensus_burn_proof, create_consensus_mint_proof, ConsensusBurnInputInfo,
    ConsensusMintOutputInfo,
};

pub struct ConsensusUnstakeRequestCallDebris {
    pub params: ConsensusStakeParamsV1,
    pub proofs: Vec<Proof>,
    pub signature_secret: SecretKey,
}

/// Struct holding necessary information to build a `Consensus::UnstakeRequestV1` contract call.
pub struct ConsensusUnstakeRequestCallBuilder {
    /// `ConsensusOwnCoin` we're given to use in this builder
    pub coin: ConsensusOwnCoin,
    /// Epoch unstaked coin is minted
    pub epoch: u64,
    /// Merkle tree of coins used to create inclusion proofs
    pub tree: MerkleTree,
    /// `ConsensusBurn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for the `ConsensusBurn_V1` zk circuit
    pub burn_pk: ProvingKey,
    /// `ConsensusMint_V1` zkas circuit ZkBinary
    pub mint_zkbin: ZkBinary,
    /// Proving key for the `ConsensusMint_V1` zk circuit
    pub mint_pk: ProvingKey,
}

impl ConsensusUnstakeRequestCallBuilder {
    pub fn build(&self) -> Result<ConsensusUnstakeRequestCallDebris> {
        debug!("Building Consensus::UnstakeRequestV1 contract call");
        assert!(self.coin.note.value != 0);

        debug!("Building anonymous input");
        let leaf_position = self.coin.leaf_position;
        let root = self.tree.root(0).unwrap();
        let merkle_path = self.tree.authentication_path(leaf_position, &root).unwrap();
        let value_blind = pallas::Scalar::random(&mut OsRng);
        let input = ConsensusBurnInputInfo {
            leaf_position,
            merkle_path,
            secret: self.coin.secret,
            note: self.coin.note.clone(),
            value_blind,
        };
        debug!("Finished building input");

        info!("Creating unstake burn proof for input");
        let value_blind = input.value_blind;
        let (burn_proof, public_inputs, signature_secret) =
            create_consensus_burn_proof(&self.burn_zkbin, &self.burn_pk, &input)?;

        let input = ConsensusInput {
            epoch: self.coin.note.epoch,
            coin: self.coin.coin,
            value_commit: public_inputs.value_commit,
            nullifier: public_inputs.nullifier,
            merkle_root: public_inputs.merkle_root,
            signature_public: public_inputs.signature_public,
        };

        debug!("Building anonymous output");
        let serial = pallas::Base::random(&mut OsRng);
        let coin_blind = pallas::Base::random(&mut OsRng);
        let public_key = public_inputs.signature_public;

        let output = ConsensusMintOutputInfo {
            value: self.coin.note.value,
            epoch: self.epoch,
            public_key,
            value_blind,
            serial,
            coin_blind,
        };
        debug!("Finished building output");

        info!("Creating stake mint proof for output");
        let (mint_proof, public_inputs) =
            create_consensus_mint_proof(&self.mint_zkbin, &self.mint_pk, &output)?;

        // Encrypted note
        let note = ConsensusNote {
            serial,
            value: output.value,
            epoch: self.epoch,
            coin_blind,
            value_blind,
            reward: 0,
            reward_blind: value_blind,
        };

        let encrypted_note = AeadEncryptedNote::encrypt(&note, &output.public_key, &mut OsRng)?;

        let output = ConsensusOutput {
            value_commit: public_inputs.value_commit,
            coin: public_inputs.coin,
            note: encrypted_note,
        };

        // We now fill this with necessary stuff
        let params = ConsensusStakeParamsV1 { input, output };
        let proofs = vec![burn_proof, mint_proof];

        // Now we should have all the params, zk proof, and signature secret.
        // We return it all and let the caller deal with it.
        let debris = ConsensusUnstakeRequestCallDebris { params, proofs, signature_secret };
        Ok(debris)
    }
}
