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
    model::{ConsensusInput, ConsensusOutput, ConsensusUnstakeReqParamsV1},
};
use darkfi_sdk::{
    crypto::{note::AeadEncryptedNote, pasta_prelude::*, Keypair, MerkleTree, SecretKey, poseidon_hash},
    pasta::pallas,
};
use log::{debug, info};
use rand::rngs::OsRng;

use crate::{
    client::common::{create_consensus_burn_proof, create_consensus_mint_proof, ConsensusBurnInputInfo, ConsensusMintOutputInfo},
    model::{
        SERIAL_PREFIX,
    },
};

pub struct ConsensusUnstakeRequestCallDebris {
    /// Payload params
    pub params: ConsensusUnstakeReqParamsV1,
    /// ZK proofs
    pub proofs: Vec<Proof>,
    /// The new output keypair (used in the minted coin)
    pub keypair: Keypair,
    /// Secret key used to sign the transaction
    pub signature_secret: SecretKey,
}

/// Struct holding necessary information to build a `Consensus::UnstakeRequestV1` contract call.
pub struct ConsensusUnstakeRequestCallBuilder {
    /// `ConsensusOwnCoin` we're given to use in this builder
    pub owncoin: ConsensusOwnCoin,
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
        info!("Building Consensus::UnstakeRequestV1 contract call");
        assert!(self.owncoin.note.value != 0);

        debug!("Building Consensus::UnstakeRequestV1 anonymous input");
        let merkle_path = self.tree.witness(self.owncoin.leaf_position, 0).unwrap();

        let input = ConsensusBurnInputInfo {
            leaf_position: self.owncoin.leaf_position,
            merkle_path,
            secret: self.owncoin.secret,
            note: self.owncoin.note.clone(),
            value_blind: pallas::Scalar::random(&mut OsRng),
        };

        debug!("Building Consensus::UnstakeRequestV1 anonymous output");
        //let output_serial = pallas::Base::random(&mut OsRng);
        // derive output secret from old secret key.
        let output_secret = poseidon_hash([self.owncoin.secret.inner()]);
        let output_keypair = Keypair::new(SecretKey::from(output_secret));
        let output_serial = poseidon_hash([SERIAL_PREFIX, self.owncoin.secret.inner(), self.owncoin.note.serial]);
        let output_coin_blind = pallas::Base::random(&mut OsRng);

        // We create a new random keypair for the output
        //let output_keypair = Keypair::random(&mut OsRng);

        let output = ConsensusMintOutputInfo {
            value: self.owncoin.note.value,
            epoch: self.epoch,
            public_key: output_keypair.public,
            value_blind: input.value_blind,
            serial: output_serial,
            coin_blind: output_coin_blind,
        };

        info!("Building Consensus::UnstakeRequestV1 Burn ZK proof");
        let (burn_proof, public_inputs, signature_secret) =
            create_consensus_burn_proof(&self.burn_zkbin, &self.burn_pk, &input)?;

        let tx_input = ConsensusInput {
            epoch: self.owncoin.note.epoch,
            value_commit: public_inputs.value_commit,
            nullifier: public_inputs.nullifier,
            merkle_root: public_inputs.merkle_root,
            signature_public: public_inputs.signature_public,
        };

        info!("Building Consensus::UnstakeRequestV1 Mint ZK proof");
        let (mint_proof, public_inputs) =
            create_consensus_mint_proof(&self.mint_zkbin, &self.mint_pk, &output)?;

        // Encrypted note
        let note = ConsensusNote {
            serial: output_serial,
            value: output.value,
            epoch: output.epoch,
            coin_blind: output_coin_blind,
            value_blind: input.value_blind,
            reward: 0,
            reward_blind: pallas::Scalar::ZERO,
        };

        let encrypted_note = AeadEncryptedNote::encrypt(&note, &output.public_key, &mut OsRng)?;

        let tx_output = ConsensusOutput {
            value_commit: public_inputs.value_commit,
            coin: public_inputs.coin,
            note: encrypted_note,
        };

        // We now fill this with necessary stuff
        let params = ConsensusUnstakeReqParamsV1 { input: tx_input, output: tx_output };

        // Construct debris
        let debris = ConsensusUnstakeRequestCallDebris {
            params,
            proofs: vec![burn_proof, mint_proof],
            keypair: output_keypair,
            signature_secret,
        };
        Ok(debris)
    }
}
