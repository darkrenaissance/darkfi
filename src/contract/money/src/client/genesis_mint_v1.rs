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

use darkfi::{
    zk::{Proof, ProvingKey},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{note::AeadEncryptedNote, pasta_prelude::*, Coin, Keypair, PublicKey, DARK_TOKEN_ID},
    pasta::pallas,
};
use log::{debug, info};
use rand::rngs::OsRng;

use crate::{
    client::{
        transfer_v1::{
            create_transfer_mint_proof, TransactionBuilderClearInputInfo,
            TransactionBuilderOutputInfo,
        },
        MoneyNote,
    },
    model::{ClearInput, MoneyTokenMintParamsV1, Output},
};

pub struct GenesisMintCallDebris {
    pub params: MoneyTokenMintParamsV1,
    pub proofs: Vec<Proof>,
}

pub struct GenesisMintRevealed {
    pub coin: Coin,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Point,
}

impl GenesisMintRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates().unwrap();
        let tokcom_coords = self.token_commit.to_affine().coordinates().unwrap();

        // NOTE: It's important to keep these in the same order
        // as the `constrain_instance` calls in the zkas code.
        vec![
            self.coin.inner(),
            *valcom_coords.x(),
            *valcom_coords.y(),
            *tokcom_coords.x(),
            *tokcom_coords.y(),
        ]
    }
}

/// Struct holding necessary information to build a `Money::GenesisMintV1` contract call.
pub struct GenesisMintCallBuilder {
    /// Caller's keypair
    pub keypair: Keypair,
    /// Amount of tokens we want to mint
    pub amount: u64,
    /// Spend hook for the output
    pub spend_hook: pallas::Base,
    /// User data for the output
    pub user_data: pallas::Base,
    /// `Mint_V1` zkas circuit ZkBinary
    pub mint_zkbin: ZkBinary,
    /// Proving key for the `Mint_V1` zk circuit
    pub mint_pk: ProvingKey,
}

impl GenesisMintCallBuilder {
    pub fn build(&self) -> Result<GenesisMintCallDebris> {
        debug!("Building Money::MintV1 contract call");
        assert!(self.amount != 0);

        // In this call, we will build one clear input and one anonymous output.
        // Only DARK_TOKEN_ID can be minted on genesis slot.
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
        let coin_blind = pallas::Base::random(&mut OsRng);

        info!("Creating token mint proof for output");
        let (proof, public_inputs) = create_transfer_mint_proof(
            &self.mint_zkbin,
            &self.mint_pk,
            &output,
            value_blind,
            token_blind,
            serial,
            self.spend_hook,
            self.user_data,
            coin_blind,
        )?;

        let note = MoneyNote {
            serial,
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

        let params = MoneyTokenMintParamsV1 { input: c_input, output: c_output };
        let debris = GenesisMintCallDebris { params, proofs: vec![proof] };
        Ok(debris)
    }
}
