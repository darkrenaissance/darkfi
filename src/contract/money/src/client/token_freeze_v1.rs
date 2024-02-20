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
use darkfi_sdk::crypto::Keypair;
use log::info;
use rand::rngs::OsRng;

use crate::model::{MoneyTokenFreezeParamsV1, TokenAttributes};

pub struct TokenFreezeCallDebris {
    pub params: MoneyTokenFreezeParamsV1,
    pub proofs: Vec<Proof>,
}

/// Struct holding necessary information to build a `Money::TokenFreezeV1` contract call.
pub struct TokenFreezeCallBuilder {
    /// Mint authority keypair
    pub mint_keypair: Keypair,
    pub token_attrs: TokenAttributes,
    /// `TokenFreeze_V1` zkas circuit ZkBinary
    pub freeze_zkbin: ZkBinary,
    /// Proving key for the `TokenFreeze_V1` zk circuit,
    pub freeze_pk: ProvingKey,
}

impl TokenFreezeCallBuilder {
    pub fn build(&self) -> Result<TokenFreezeCallDebris> {
        info!("Building Money::TokenFreezeV1 contract call");

        // For the TokenFreeze call, we just need to produce a valid signature,
        // and enforce the correct derivation inside ZK.
        let prover_witnesses = vec![
            // Token attributes
            Witness::Base(Value::known(self.token_attrs.auth_parent.inner())),
            Witness::Base(Value::known(self.token_attrs.blind.inner())),
            // Secret key used by mint
            Witness::Base(Value::known(self.mint_keypair.secret.inner())),
        ];

        let mint_pubkey = self.mint_keypair.public;
        let token_id = self.token_attrs.to_token_id();

        let public_inputs = vec![mint_pubkey.x(), mint_pubkey.y(), token_id.inner()];
        //darkfi::zk::export_witness_json("witness.json", &prover_witnesses, &public_inputs);

        let circuit = ZkCircuit::new(prover_witnesses, &self.freeze_zkbin);
        let proof = Proof::create(&self.freeze_pk, &[circuit], &public_inputs, &mut OsRng)?;

        let params = MoneyTokenFreezeParamsV1 { mint_public: self.mint_keypair.public, token_id };
        let debris = TokenFreezeCallDebris { params, proofs: vec![proof] };
        Ok(debris)
    }
}
