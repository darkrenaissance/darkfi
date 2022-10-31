/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use log::error;

use rand::rngs::OsRng;

use crate::{
    crypto::{
        leadcoin::LeadCoin,
        proof::{Proof, ProvingKey, VerifyingKey},
        types::*,
    },
    Result, VerifyFailed, VerifyResult,
};

#[allow(clippy::too_many_arguments)]
pub fn create_lead_proof(pk: &ProvingKey, coin: LeadCoin) -> Result<Proof> {
    let contract = coin.create_contract();
    let public_inputs = coin.public_inputs();
    let proof = Proof::create(pk, &[contract], &public_inputs, &mut OsRng)?;
    Ok(proof)
}

pub fn verify_lead_proof(
    vk: &VerifyingKey,
    proof: &Proof,
    public_inputs: &[DrkCircuitField],
) -> VerifyResult<()> {
    match proof.verify(vk, public_inputs) {
        Ok(()) => Ok(()),
        Err(e) => {
            error!("lead verification failed: {}", e);
            Err(VerifyFailed::InternalError("lead verification failure".to_string()))
        }
    }
}
