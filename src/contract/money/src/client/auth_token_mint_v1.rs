/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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
use darkfi_sdk::{crypto::Keypair, pasta::pallas};
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::{CoinAttributes, MoneyAuthTokenMintParamsV1, TokenAttributes};

pub struct AuthTokenMintCallDebris {
    pub params: MoneyAuthTokenMintParamsV1,
    pub proofs: Vec<Proof>,
}

/// Struct holding necessary information to build a `Money::AuthTokenMintV1` contract call.
pub struct AuthTokenMintCallBuilder {
    /// Coin attributes
    pub coin_attrs: CoinAttributes,
    /// Token attributes
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
        debug!(target: "contract::money::client::auth_token_mint", "Building Money::AuthTokenMintV1 contract call");

        // Create the proof
        let (public_x, public_y) = self.coin_attrs.public_key.xy();
        let prover_witnesses = vec![
            // Secret key used by the mint authority
            Witness::Base(Value::known(self.mint_keypair.secret.inner())),
            // Token attributes
            Witness::Base(Value::known(self.token_attrs.auth_parent.inner())),
            Witness::Base(Value::known(self.token_attrs.blind.inner())),
            // Coin attributes
            Witness::Base(Value::known(public_x)),
            Witness::Base(Value::known(public_y)),
            Witness::Base(Value::known(pallas::Base::from(self.coin_attrs.value))),
            Witness::Base(Value::known(self.coin_attrs.spend_hook.inner())),
            Witness::Base(Value::known(self.coin_attrs.user_data)),
            Witness::Base(Value::known(self.coin_attrs.blind.inner())),
        ];

        let mint_pubkey = self.mint_keypair.public;
        let token_id = self.token_attrs.to_token_id();
        let coin = self.coin_attrs.to_coin();

        let public_inputs = vec![
            mint_pubkey.x(),
            mint_pubkey.y(),
            self.token_attrs.auth_parent.inner(),
            token_id.inner(),
            coin.inner(),
        ];

        //darkfi::zk::export_witness_json("proof/witness/auth_token_mint_v1.json", &prover_witnesses, &public_inputs);
        let circuit = ZkCircuit::new(prover_witnesses, &self.auth_mint_zkbin);
        let proof = Proof::create(&self.auth_mint_pk, &[circuit], &public_inputs, &mut OsRng)?;

        let params = MoneyAuthTokenMintParamsV1 { token_id, mint_pubkey };
        let debris = AuthTokenMintCallDebris { params, proofs: vec![proof] };
        Ok(debris)
    }
}
