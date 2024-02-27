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

use darkfi::{
    zk::{Proof, ProvingKey},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    blockchain::expected_reward,
    crypto::{
        ecvrf::VrfProof, note::AeadEncryptedNote, pasta_prelude::*, Blind, FuncId, PublicKey,
        SecretKey,
    },
    pasta::pallas,
};
use log::{debug, info};
use rand::rngs::OsRng;

use crate::{
    client::{
        transfer_v1::{
            proof::create_transfer_mint_proof, TransferCallClearInput, TransferCallOutput,
        },
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
    /// Caller's secret key, used for signing and VRF proof generation
    pub secret: SecretKey,
    /// Reward recipient's public key
    pub recipient: PublicKey,
    /// Rewarded block height
    pub block_height: u64,
    /// Extending fork last proposal/block nonce
    pub last_nonce: u64,
    /// Extending fork second to last proposal/block hash
    pub fork_previous_hash: blake3::Hash,
    /// Merkle tree of coins used to create inclusion proofs
    /// Spend hook for the output
    pub spend_hook: FuncId,
    /// User data for the output
    pub user_data: pallas::Base,
    /// `Mint_V1` zkas circuit ZkBinary
    pub mint_zkbin: ZkBinary,
    /// Proving key for the `Mint_V1` zk circuit
    pub mint_pk: ProvingKey,
}

impl PoWRewardCallBuilder {
    fn _build(&self, value: u64) -> Result<PoWRewardCallDebris> {
        debug!("Building Money::MintV1 contract call");

        // In this call, we will build one clear input and one anonymous output.
        // Only DARK_TOKEN_ID can be minted as PoW reward.
        let token_id = *DARK_TOKEN_ID;

        let input = TransferCallClearInput { value, token_id, signature_secret: self.secret };

        let output = TransferCallOutput {
            public_key: self.recipient,
            value,
            token_id,
            spend_hook: FuncId::none(),
            user_data: pallas::Base::ZERO,
            blind: Blind::random(&mut OsRng),
        };

        // We just create the commitment blinds here. We simply encofce
        // that the clear input and the anon output have the same commitments.
        // Not sure if this can be avoided, but also is it really necessary to avoid?
        let value_blind = Blind::random(&mut OsRng);
        let token_blind = Blind::random(&mut OsRng);

        let c_input = ClearInput {
            value: input.value,
            token_id: input.token_id,
            value_blind,
            token_blind,
            signature_public: PublicKey::from_secret(input.signature_secret),
        };

        let coin_blind = Blind::random(&mut OsRng);

        info!("Creating token mint proof for output");
        let (proof, public_inputs) = create_transfer_mint_proof(
            &self.mint_zkbin,
            &self.mint_pk,
            &output,
            value_blind,
            token_blind,
            self.spend_hook,
            self.user_data,
            coin_blind,
        )?;

        let note = MoneyNote {
            value: output.value,
            token_id: output.token_id,
            spend_hook: self.spend_hook,
            user_data: self.user_data,
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

        info!("Building Consensus::ProposalV1 VRF proof");
        let mut vrf_input = Vec::with_capacity(32 + blake3::OUT_LEN + 32);
        vrf_input.extend_from_slice(&pallas::Base::from(self.last_nonce).to_repr());
        vrf_input.extend_from_slice(self.fork_previous_hash.as_bytes());
        vrf_input.extend_from_slice(&pallas::Base::from(self.block_height).to_repr());
        let vrf_proof = VrfProof::prove(self.secret, &vrf_input);

        let params = MoneyPoWRewardParamsV1 { input: c_input, output: c_output, vrf_proof };
        let debris = PoWRewardCallDebris { params, proofs: vec![proof] };
        Ok(debris)
    }

    pub fn build(&self) -> Result<PoWRewardCallDebris> {
        let reward = expected_reward(self.block_height);
        assert!(reward != 0);
        self._build(reward)
    }

    /// This function should only be used for testing, as PoW reward values are predefined
    pub fn build_with_custom_reward(&self, reward: u64) -> Result<PoWRewardCallDebris> {
        self._build(reward)
    }
}
