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
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{note::AeadEncryptedNote, pasta_prelude::*, pedersen_commitment_u64, Blind, Keypair},
    pasta::pallas,
};
use log::debug;
use rand::rngs::OsRng;

use crate::{
    client::MoneyNote,
    model::{CoinAttributes, MoneyAuthTokenMintParamsV1, TokenAttributes},
};

pub struct AuthTokenMintCallDebris {
    pub params: MoneyAuthTokenMintParamsV1,
    pub proofs: Vec<Proof>,
}

/// Struct holding necessary information to build a `Money::AuthTokenMintV1` contract call.
pub struct AuthTokenMintCallBuilder {
    pub coin_attrs: CoinAttributes,
    pub token_attrs: TokenAttributes,

    /// Mint authority keypair
    pub mint_keypair: Keypair,

    /// `AuthTokenMint_V1` zkas circuit ZkBinary
    pub auth_mint_zkbin: ZkBinary,
    /// Proving key for the `AuthTokenMint_V1` zk circuit,
    pub auth_mint_pk: ProvingKey,
}

impl AuthTokenMintCallBuilder {
    pub fn build(&self) -> Result<AuthTokenMintCallDebris> {
        debug!("Building Money::AuthTokenMintV1 contract call");

        let value_blind = Blind::random(&mut OsRng);
        let value_commit = pedersen_commitment_u64(self.coin_attrs.value, value_blind);

        // Create the proof

        let (public_x, public_y) = self.coin_attrs.public_key.xy();

        let prover_witnesses = vec![
            // Coin attributes
            Witness::Base(Value::known(public_x)),
            Witness::Base(Value::known(public_y)),
            Witness::Base(Value::known(pallas::Base::from(self.coin_attrs.value))),
            Witness::Base(Value::known(self.coin_attrs.spend_hook.inner())),
            Witness::Base(Value::known(self.coin_attrs.user_data)),
            Witness::Base(Value::known(self.coin_attrs.blind.inner())),
            // Token attributes
            Witness::Base(Value::known(self.token_attrs.auth_parent.inner())),
            Witness::Base(Value::known(self.token_attrs.blind.inner())),
            // Secret key used by mint
            Witness::Base(Value::known(self.mint_keypair.secret.inner())),
            // Random blinding factor for the value commitment
            Witness::Scalar(Value::known(value_blind.inner())),
        ];

        let mint_pubkey = self.mint_keypair.public;
        let value_coords = value_commit.to_affine().coordinates().unwrap();

        let public_inputs = vec![
            mint_pubkey.x(),
            mint_pubkey.y(),
            self.token_attrs.to_token_id().inner(),
            self.coin_attrs.to_coin().inner(),
            *value_coords.x(),
            *value_coords.y(),
        ];

        //darkfi::zk::export_witness_json("proof/witness/auth_token_mint_v1.json", &prover_witnesses, &public_inputs);
        let circuit = ZkCircuit::new(prover_witnesses, &self.auth_mint_zkbin);
        let proof = Proof::create(&self.auth_mint_pk, &[circuit], &public_inputs, &mut OsRng)?;

        // Create the note

        let note = MoneyNote {
            value: self.coin_attrs.value,
            token_id: self.coin_attrs.token_id,
            spend_hook: self.coin_attrs.spend_hook,
            user_data: self.coin_attrs.user_data,
            coin_blind: self.coin_attrs.blind,
            value_blind,
            token_blind: Blind::ZERO,
            memo: vec![],
        };

        let enc_note = AeadEncryptedNote::encrypt(&note, &self.coin_attrs.public_key, &mut OsRng)?;

        let params = MoneyAuthTokenMintParamsV1 {
            token_id: self.token_attrs.to_token_id(),
            value_commit,
            enc_note,
            mint_pubkey,
        };
        let debris = AuthTokenMintCallDebris { params, proofs: vec![proof] };
        Ok(debris)
    }
}
