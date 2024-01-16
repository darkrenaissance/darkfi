/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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
    client::ConsensusNote,
    model::{ClearInput, ConsensusOutput},
};
use darkfi_sdk::{
    crypto::{note::AeadEncryptedNote, pasta_prelude::*, Keypair, PublicKey, DARK_TOKEN_ID},
    pasta::pallas,
};
use log::{debug, info};
use rand::rngs::OsRng;

use crate::{
    client::common::{create_consensus_mint_proof, ConsensusMintOutputInfo},
    model::ConsensusGenesisStakeParamsV1,
};

pub struct ConsensusGenesisStakeCallDebris {
    pub params: ConsensusGenesisStakeParamsV1,
    pub proofs: Vec<Proof>,
}

/// Struct holding necessary information to build a `Consensus::GenesisStakeV1` contract call.
pub struct ConsensusGenesisStakeCallBuilder {
    /// Signer keypair, this pubkey is in the clear input, and used to sign the tx.
    pub keypair: Keypair,
    /// Output pubkey, to whom the minted coin goes. The secret should be managed externally.
    pub recipient: PublicKey,
    /// Amount of tokens we want to mint and stake
    pub amount: u64,
    /// `ConsensusMint_V1` zkas circuit ZkBinary
    pub mint_zkbin: ZkBinary,
    /// Proving key for the `ConsensusMint_V1` zk circuit
    pub mint_pk: ProvingKey,
}

impl ConsensusGenesisStakeCallBuilder {
    pub fn build(&self) -> Result<ConsensusGenesisStakeCallDebris> {
        // We just create the pedersen commitment blinds here. We simply
        // enforce that the clear input and the anon output have the same
        // commitments.
        let value_blind = pallas::Scalar::random(&mut OsRng);
        let token_blind = pallas::Base::random(&mut OsRng);
        let reward_blind = pallas::Scalar::random(&mut OsRng);

        // FIXME: The coin's serial number here is arbitrary, and allows grinding attacks.
        let serial = pallas::Base::random(&mut OsRng);

        self.build_with_params(value_blind, token_blind, reward_blind, serial)
    }

    pub fn build_with_params(
        &self,
        value_blind: pallas::Scalar,
        token_blind: pallas::Base,
        reward_blind: pallas::Scalar,
        serial: pallas::Base,
    ) -> Result<ConsensusGenesisStakeCallDebris> {
        debug!("Building Consensus::GenesisStakeV1 contract call");
        let value = self.amount;
        assert!(value != 0);

        // In this call, we will build one clear input and one anonymous output.
        // Only DARK_TOKEN_ID can be minted and staked on genesis slot.
        let token_id = *DARK_TOKEN_ID;

        // With genesis, our epoch is 0.
        let epoch = 0;

        // Parameters for the clear input
        let c_input = ClearInput {
            value,
            token_id,
            value_blind,
            token_blind,
            signature_public: self.keypair.public,
        };

        // Parameters for the anonymous output
        let output = ConsensusMintOutputInfo {
            value,
            epoch,
            public_key: self.recipient,
            value_blind,
            serial,
        };

        info!("Creating genesis stake mint proof for output");
        let (proof, public_inputs) =
            create_consensus_mint_proof(&self.mint_zkbin, &self.mint_pk, &output)?;

        // Encrypted note
        let note = ConsensusNote {
            serial,
            value: output.value,
            epoch,
            value_blind,
            reward: 0,
            reward_blind,
        };

        let encrypted_note = AeadEncryptedNote::encrypt(&note, &self.recipient, &mut OsRng)?;

        let output = ConsensusOutput {
            value_commit: public_inputs.value_commit,
            coin: public_inputs.coin,
            note: encrypted_note,
        };

        // We now fill this with necessary stuff
        let params = ConsensusGenesisStakeParamsV1 { input: c_input, output };
        let proofs = vec![proof];

        // Now we should have all the params and zk proof.
        // We return it all and let the caller deal with it.
        let debris = ConsensusGenesisStakeCallDebris { params, proofs };
        Ok(debris)
    }
}
