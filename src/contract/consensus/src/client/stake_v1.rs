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
    client::{ConsensusNote, OwnCoin},
    model::{ConsensusInput, ConsensusOutput, ConsensusStakeParamsV1},
};
use darkfi_sdk::{
    crypto::{
        note::AeadEncryptedNote, pasta_prelude::*, MerkleNode, Nullifier, PublicKey, SecretKey,
        DARK_TOKEN_ID,
    },
    pasta::pallas,
};
use log::{debug, info};
use rand::rngs::OsRng;

use crate::client::common::{create_consensus_mint_proof, ConsensusMintOutputInfo};

pub struct ConsensusStakeCallDebris {
    pub params: ConsensusStakeParamsV1,
    pub proofs: Vec<Proof>,
    pub signature_secret: SecretKey,
}

/// Struct holding necessary information to build a `Consensus::StakeV1` contract call.
pub struct ConsensusStakeCallBuilder {
    /// `OwnCoin` we're given to use in this builder
    pub coin: OwnCoin,
    /// Epoch staked coin is minted
    pub epoch: u64,
    /// Blinding factor for value commitment
    pub value_blind: pallas::Scalar,
    /// Revealed nullifier
    pub nullifier: Nullifier,
    /// Revealed Merkle root
    pub merkle_root: MerkleNode,
    /// `ConsensusMint_V1` zkas circuit ZkBinary
    pub mint_zkbin: ZkBinary,
    /// Proving key for the `ConsensusMint_V1` zk circuit
    pub mint_pk: ProvingKey,
}

impl ConsensusStakeCallBuilder {
    pub fn build(&self) -> Result<ConsensusStakeCallDebris> {
        debug!("Building Consensus::StakeV1 contract call");
        assert!(self.coin.note.value != 0);
        assert!(self.coin.note.token_id == *DARK_TOKEN_ID);

        debug!("Building anonymous output");
        let serial = pallas::Base::random(&mut OsRng);
        let coin_blind = pallas::Base::random(&mut OsRng);
        let public_key = PublicKey::from_secret(self.coin.secret);

        let output = ConsensusMintOutputInfo {
            value: self.coin.note.value,
            epoch: self.epoch,
            public_key,
            value_blind: self.value_blind,
            serial,
            coin_blind,
        };
        debug!("Finished building output");

        info!("Creating stake mint proof for output");
        let (proof, public_inputs) =
            create_consensus_mint_proof(&self.mint_zkbin, &self.mint_pk, &output)?;

        // Encrypted note
        let note = ConsensusNote {
            serial,
            value: output.value,
            epoch: self.epoch,
            coin_blind,
            value_blind: self.value_blind,
            reward: 0,
            reward_blind: self.value_blind,
        };

        let encrypted_note = AeadEncryptedNote::encrypt(&note, &output.public_key, &mut OsRng)?;

        let output = ConsensusOutput {
            value_commit: public_inputs.value_commit,
            coin: public_inputs.coin,
            note: encrypted_note,
        };

        let input = ConsensusInput {
            epoch: self.epoch,
            coin: self.coin.coin,
            value_commit: public_inputs.value_commit,
            nullifier: self.nullifier,
            merkle_root: self.merkle_root,
            signature_public: public_key,
        };

        // We now fill this with necessary stuff
        let params = ConsensusStakeParamsV1 { input, output };
        let proofs = vec![proof];

        // Now we should have all the params and zk proof.
        // We return it all and let the caller deal with it.
        let debris =
            ConsensusStakeCallDebris { params, proofs, signature_secret: self.coin.secret };
        Ok(debris)
    }
}
