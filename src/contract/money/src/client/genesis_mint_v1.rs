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
    ClientFailed, Result,
};
use darkfi_sdk::{
    crypto::{note::AeadEncryptedNote, pasta_prelude::*, Blind, FuncId, PublicKey},
    pasta::pallas,
};
use log::debug;
use rand::{prelude::SliceRandom, rngs::OsRng};

use crate::{
    client::{
        compute_remainder_blind,
        transfer_v1::{proof::create_transfer_mint_proof, TransferCallOutput},
        MoneyNote,
    },
    model::{ClearInput, Coin, MoneyGenesisMintParamsV1, Output, DARK_TOKEN_ID},
};

pub struct GenesisMintCallDebris {
    pub params: MoneyGenesisMintParamsV1,
    pub proofs: Vec<Proof>,
}

pub struct GenesisMintRevealed {
    pub coin: Coin,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
}

impl GenesisMintRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates().unwrap();

        // NOTE: It's important to keep these in the same order
        // as the `constrain_instance` calls in the zkas code.
        vec![self.coin.inner(), *valcom_coords.x(), *valcom_coords.y(), self.token_commit]
    }
}

/// Struct holding necessary information to build a `Money::GenesisMintV1` contract call.
pub struct GenesisMintCallBuilder {
    /// Caller's public key, corresponding to the one used in the signature
    pub signature_public: PublicKey,
    /// Vector containing each output value we want to mint
    pub amounts: Vec<u64>,
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

impl GenesisMintCallBuilder {
    pub fn build(&self) -> Result<GenesisMintCallDebris> {
        debug!(target: "contract::money::client::genesis_mint", "Building Money::MintV1 contract call");
        let value = self.amounts.iter().sum();
        if value == 0 {
            return Err(ClientFailed::InvalidAmount(value).into())
        }

        // In this call, we will build one clear input and one anonymous output.
        // Only DARK_TOKEN_ID can be minted on genesis block.
        let token_id = *DARK_TOKEN_ID;

        // Building the clear input using random blinds
        let value_blind = Blind::random(&mut OsRng);
        let token_blind = Blind::random(&mut OsRng);
        let input = ClearInput {
            value,
            token_id,
            value_blind,
            token_blind,
            signature_public: self.signature_public,
        };

        // Grab the public key, spend hook and user data to use in the outputs
        let public_key = self.recipient.unwrap_or(self.signature_public);
        let spend_hook = self.spend_hook.unwrap_or(FuncId::none());
        let user_data = self.user_data.unwrap_or(pallas::Base::ZERO);

        // Shuffle the amounts vector so our outputs are not in order
        let mut amounts = self.amounts.clone();
        amounts.shuffle(&mut OsRng);

        // Building the anonymous outputs
        let input_blinds = vec![value_blind];
        let mut output_blinds = Vec::with_capacity(amounts.len());
        let mut outputs = Vec::with_capacity(amounts.len());
        let mut proofs = Vec::with_capacity(amounts.len());
        for (i, amount) in amounts.iter().enumerate() {
            let value_blind = if i == amounts.len() - 1 {
                compute_remainder_blind(&input_blinds, &output_blinds)
            } else {
                Blind::random(&mut OsRng)
            };
            output_blinds.push(value_blind);

            let output = TransferCallOutput {
                public_key,
                value: *amount,
                token_id,
                spend_hook,
                user_data,
                blind: Blind::random(&mut OsRng),
            };

            debug!(target: "contract::money::client::genesis_mint", "Creating token mint proof for output {i}");
            let (proof, public_inputs) = create_transfer_mint_proof(
                &self.mint_zkbin,
                &self.mint_pk,
                &output,
                value_blind,
                token_blind,
                spend_hook,
                user_data,
                output.blind,
            )?;
            proofs.push(proof);

            let note = MoneyNote {
                value: output.value,
                token_id: output.token_id,
                spend_hook,
                user_data,
                coin_blind: output.blind,
                value_blind,
                token_blind,
                memo: vec![],
            };

            let encrypted_note = AeadEncryptedNote::encrypt(&note, &public_key, &mut OsRng)?;

            let output = Output {
                value_commit: public_inputs.value_commit,
                token_commit: public_inputs.token_commit,
                coin: public_inputs.coin,
                note: encrypted_note,
            };

            outputs.push(output);
        }

        let params = MoneyGenesisMintParamsV1 { input, outputs };
        let debris = GenesisMintCallDebris { params, proofs };
        Ok(debris)
    }
}
