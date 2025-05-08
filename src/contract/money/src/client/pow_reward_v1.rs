/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use darkfi::{
    zk::{Proof, ProvingKey},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    blockchain::expected_reward,
    crypto::{note::AeadEncryptedNote, pasta_prelude::*, Blind, FuncId, PublicKey},
    pasta::pallas,
};
use log::debug;
use rand::rngs::OsRng;

use crate::{
    client::{
        transfer_v1::{proof::create_transfer_mint_proof, TransferCallOutput},
        MoneyNote,
    },
    model::{ClearInput, Coin, MoneyPoWRewardParamsV1, Output, DARK_TOKEN_ID},
};

pub struct PoWRewardCallDebris {
    pub params: MoneyPoWRewardParamsV1,
    pub proofs: Vec<Proof>,
}

pub struct PoWRewardRevealed {
    pub coin: Coin,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
}

impl PoWRewardRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates().unwrap();

        // NOTE: It's important to keep these in the same order
        // as the `constrain_instance` calls in the zkas code.
        vec![self.coin.inner(), *valcom_coords.x(), *valcom_coords.y(), self.token_commit]
    }
}

/// Struct holding necessary information to build a `Money::PoWRewardV1` contract call.
pub struct PoWRewardCallBuilder {
    /// Caller's public key, corresponding to the one used in the signature
    pub signature_public: PublicKey,
    /// Rewarded block height
    pub block_height: u32,
    /// Rewarded block transactions paid fees
    pub fees: u64,
    /// Optional recipient's public key, in case we want to mint to a different address
    pub recipient: Option<PublicKey>,
    /// Optional contract spend hook to use in the output
    pub spend_hook: Option<FuncId>,
    /// Optional user data to use in the output
    pub user_data: Option<pallas::Base>,
    /// `Mint_V1` zkas circuit ZkBinary
    pub mint_zkbin: ZkBinary,
    /// Proving key for the `Mint_V1` zk circuit
    pub mint_pk: ProvingKey,
}

impl PoWRewardCallBuilder {
    fn _build(&self, value: u64) -> Result<PoWRewardCallDebris> {
        debug!(target: "contract::money::client::pow_reward", "Building Money::PoWRewardV1 contract call");

        // In this call, we will build one clear input and one anonymous output.
        // Only DARK_TOKEN_ID can be minted as PoW reward.
        let token_id = *DARK_TOKEN_ID;

        // Building the clear input using random blinds
        let value_blind = Blind::random(&mut OsRng);
        let token_blind = Blind::random(&mut OsRng);
        let coin_blind = Blind::random(&mut OsRng);
        let c_input = ClearInput {
            value,
            token_id,
            value_blind,
            token_blind,
            signature_public: self.signature_public,
        };

        // Grab the spend hook and user data to use in the output
        let spend_hook = self.spend_hook.unwrap_or(FuncId::none());
        let user_data = self.user_data.unwrap_or(pallas::Base::ZERO);

        // Building the anonymous output
        let output = TransferCallOutput {
            public_key: self.recipient.unwrap_or(self.signature_public),
            value,
            token_id,
            spend_hook,
            user_data,
            blind: Blind::random(&mut OsRng),
        };

        debug!(target: "contract::money::client::pow_reward", "Creating token mint proof for output");
        let (proof, public_inputs) = create_transfer_mint_proof(
            &self.mint_zkbin,
            &self.mint_pk,
            &output,
            value_blind,
            token_blind,
            spend_hook,
            user_data,
            coin_blind,
        )?;

        let note = MoneyNote {
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

        let c_output = Output {
            value_commit: public_inputs.value_commit,
            token_commit: public_inputs.token_commit,
            coin: public_inputs.coin,
            note: encrypted_note,
        };

        let params = MoneyPoWRewardParamsV1 { input: c_input, output: c_output };
        let debris = PoWRewardCallDebris { params, proofs: vec![proof] };
        Ok(debris)
    }

    pub fn build(&self) -> Result<PoWRewardCallDebris> {
        let reward = expected_reward(self.block_height) + self.fees;
        self._build(reward)
    }

    /// This function should only be used for testing, as PoW reward values are predefined
    pub fn build_with_custom_reward(&self, reward: u64) -> Result<PoWRewardCallDebris> {
        self._build(reward + self.fees)
    }
}
