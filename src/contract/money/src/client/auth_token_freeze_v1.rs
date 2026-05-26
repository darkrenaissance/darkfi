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
use darkfi_sdk::crypto::Keypair;
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::{MoneyAuthTokenFreezeParamsV1, TokenAttributes};

pub struct AuthTokenFreezeCallDebris {
    pub params: MoneyAuthTokenFreezeParamsV1,
    pub proofs: Vec<Proof>,
}

/// Struct holding necessary information to build a `Money::AuthTokenFreezeV1` contract call.
pub struct AuthTokenFreezeCallBuilder {
    /// Mint authority keypair
    pub mint_keypair: Keypair,
    /// Token attributes
    pub token_attrs: TokenAttributes,
    /// `AuthTokenFreeze_V1` zkas circuit ZkBinary
    pub auth_freeze_zkbin: ZkBinary,
    /// Proving key for the `AuthTokenFreeze_V1` zk circuit,
    pub auth_freeze_pk: ProvingKey,
}

impl AuthTokenFreezeCallBuilder {
    pub fn build(&self) -> Result<AuthTokenFreezeCallDebris> {
        debug!(target: "contract::money::client::auth_token_freeze", "Building Money::AuthTokenFreezeV1 contract call");

        // For the AuthTokenFreeze call, we just need to produce a valid signature,
        // and enforce the correct derivation inside ZK.
        let prover_witnesses = vec![
            // Secret key used by the mint authority
            Witness::Base(Value::known(self.mint_keypair.secret.inner())),
            // Token attributes
            Witness::Base(Value::known(self.token_attrs.auth_parent.inner())),
            Witness::Base(Value::known(self.token_attrs.blind.inner())),
        ];

        let mint_public = self.mint_keypair.public;
        let token_id = self.token_attrs.to_token_id();

        let public_inputs = vec![
            mint_public.x(),
            mint_public.y(),
            self.token_attrs.auth_parent.inner(),
            token_id.inner(),
        ];
        //darkfi::zk::export_witness_json("proof/witness/auth_token_freeze_v1.json", &prover_witnesses, &public_inputs);
        let circuit = ZkCircuit::new(prover_witnesses, &self.auth_freeze_zkbin);
        let proof = Proof::create(&self.auth_freeze_pk, &[circuit], &public_inputs, &mut OsRng)?;

        let params = MoneyAuthTokenFreezeParamsV1 { mint_public, token_id };
        let debris = AuthTokenFreezeCallDebris { params, proofs: vec![proof] };
        Ok(debris)
    }
}
