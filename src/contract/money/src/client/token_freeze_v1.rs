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
    crypto::{Keypair, PublicKey, TokenId},
    pasta::pallas,
};
use log::{debug, info};
use rand::rngs::OsRng;

use crate::model::MoneyTokenFreezeParamsV1;

pub struct TokenFreezeCallDebris {
    pub params: MoneyTokenFreezeParamsV1,
    pub proofs: Vec<Proof>,
}

pub struct TokenFreezeRevealed {
    pub signature_public: PublicKey,
    pub token_id: TokenId,
}

impl TokenFreezeRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let (sig_x, sig_y) = self.signature_public.xy();
        vec![sig_x, sig_y, self.token_id.inner()]
    }
}

/// Struct holding necessary information to build a `Money::TokenFreezeV1` contract call.
pub struct TokenFreezeCallBuilder {
    /// Mint authority keypair
    pub mint_authority: Keypair,
    /// `TokenFreeze_V1` zkas circuit ZkBinary
    pub token_freeze_zkbin: ZkBinary,
    /// Proving key for the `TokenFreeze_V1` zk circuit,
    pub token_freeze_pk: ProvingKey,
}

impl TokenFreezeCallBuilder {
    pub fn build(&self) -> Result<TokenFreezeCallDebris> {
        info!("Building Money::TokenFreezeV1 contract call");

        // For the TokenFreeze call, we just need to produce a valid signature,
        // and enforce the correct derivation inside ZK.
        debug!("Creating token freeze ZK proof");
        let (proof, _public_inputs) = create_token_freeze_proof(
            &self.token_freeze_zkbin,
            &self.token_freeze_pk,
            &self.mint_authority,
        )?;

        let params = MoneyTokenFreezeParamsV1 { signature_public: self.mint_authority.public };
        let debris = TokenFreezeCallDebris { params, proofs: vec![proof] };
        Ok(debris)
    }
}

pub(crate) fn create_token_freeze_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    mint_authority: &Keypair,
) -> Result<(Proof, TokenFreezeRevealed)> {
    let token_id = TokenId::derive(mint_authority.secret);

    let public_inputs = TokenFreezeRevealed { signature_public: mint_authority.public, token_id };

    let prover_witnesses = vec![Witness::Base(Value::known(mint_authority.secret.inner()))];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
