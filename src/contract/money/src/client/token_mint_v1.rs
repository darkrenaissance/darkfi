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
use darkfi_sdk::pasta::pallas;
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::{CoinAttributes, MoneyTokenMintParamsV1, TokenAttributes};

pub struct TokenMintCallDebris {
    pub params: MoneyTokenMintParamsV1,
    pub proofs: Vec<Proof>,
}

/// Struct holding necessary information to build a `Money::TokenMintV1` contract call.
pub struct TokenMintCallBuilder {
    pub coin_attrs: CoinAttributes,
    pub token_attrs: TokenAttributes,

    /// `TokenMint_V1` zkas circuit ZkBinary
    pub mint_zkbin: ZkBinary,
    /// Proving key for the `TokenMint_V1` zk circuit,
    pub mint_pk: ProvingKey,
}

impl TokenMintCallBuilder {
    pub fn build(&self) -> Result<TokenMintCallDebris> {
        debug!(target: "contract::money::client::token_mint", "Building Money::TokenMintV1 contract call");
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
            Witness::Base(Value::known(self.token_attrs.user_data)),
            Witness::Base(Value::known(self.token_attrs.blind.inner())),
        ];

        let coin = self.coin_attrs.to_coin();

        let public_inputs = vec![self.token_attrs.auth_parent.inner(), coin.inner()];

        //darkfi::zk::export_witness_json( "proof/witness/token_mint_v1.json", &prover_witnesses, &public_inputs);
        let circuit = ZkCircuit::new(prover_witnesses, &self.mint_zkbin);
        let proof = Proof::create(&self.mint_pk, &[circuit], &public_inputs, &mut OsRng)?;

        let params = MoneyTokenMintParamsV1 { coin };
        let debris = TokenMintCallDebris { params, proofs: vec![proof] };
        Ok(debris)
    }
}
