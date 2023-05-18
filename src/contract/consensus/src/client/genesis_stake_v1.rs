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
    client::{transfer_v1::TransactionBuilderClearInputInfo, MoneyNote},
    model::{ClearInput, Output},
};
use darkfi_sdk::{
    crypto::{
        note::AeadEncryptedNote, pasta_prelude::*, Keypair, PublicKey, CONSENSUS_CONTRACT_ID,
        DARK_TOKEN_ID,
    },
    pasta::pallas,
};
use log::{debug, info};
use rand::rngs::OsRng;

use crate::{
    client::stake_v1::{create_stake_mint_proof, TransactionBuilderOutputInfo},
    model::ConsensusGenesisStakeParamsV1,
};

pub struct ConsensusGenesisStakeCallDebris {
    pub params: ConsensusGenesisStakeParamsV1,
    pub proofs: Vec<Proof>,
}

/// Struct holding necessary information to build a `Consensus::GenesisStakeV1` contract call.
pub struct ConsensusGenesisStakeCallBuilder {
    /// Caller's keypair
    pub keypair: Keypair,
    /// Amount of tokens we want to mint and stake
    pub amount: u64,
    /// `Mint_V1` zkas circuit ZkBinary
    pub mint_zkbin: ZkBinary,
    /// Proving key for the `Mint_V1` zk circuit
    pub mint_pk: ProvingKey,
}

impl ConsensusGenesisStakeCallBuilder {
    pub fn build(&self) -> Result<ConsensusGenesisStakeCallDebris> {
        debug!("Building Consensus::GenesisStakeV1 contract call");
        assert!(self.amount != 0);

        // In this call, we will build one clear input and one anonymous output.
        // Only DARK_TOKEN_ID can be minted and staked on genesis slot.
        let token_id = *DARK_TOKEN_ID;

        let input = TransactionBuilderClearInputInfo {
            value: self.amount,
            token_id,
            signature_secret: self.keypair.secret,
        };

        let output = TransactionBuilderOutputInfo {
            value: self.amount,
            token_id,
            public_key: self.keypair.public,
        };

        // We just create the pedersen commitment blinds here. We simply
        // enforce that the clear input and the anon output have the same
        // commitments. Not sure if this can be avoided, but also is it
        // really necessary to avoid?
        let value_blind = pallas::Scalar::random(&mut OsRng);
        let token_blind = pallas::Scalar::random(&mut OsRng);

        let c_input = ClearInput {
            value: input.value,
            token_id: input.token_id,
            value_blind,
            token_blind,
            signature_public: PublicKey::from_secret(input.signature_secret),
        };

        let serial = pallas::Base::random(&mut OsRng);
        let spend_hook = CONSENSUS_CONTRACT_ID.inner();
        let user_data = pallas::Base::random(&mut OsRng);
        let coin_blind = pallas::Base::random(&mut OsRng);

        info!("Creating genesis stake mint proof for output");
        let (proof, public_inputs) = create_stake_mint_proof(
            &self.mint_zkbin,
            &self.mint_pk,
            &output,
            value_blind,
            token_blind,
            serial,
            spend_hook,
            user_data,
            coin_blind,
        )?;

        // Encrypted note
        let note = MoneyNote {
            serial,
            value: output.value,
            token_id: output.token_id,
            spend_hook,
            user_data,
            coin_blind,
            value_blind,
            token_blind,
            memo: vec![],
        };

        let encrypted_note = AeadEncryptedNote::encrypt(&note, &output.public_key, &mut OsRng)?;

        let output = Output {
            value_commit: public_inputs.value_commit,
            token_commit: public_inputs.token_commit,
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
