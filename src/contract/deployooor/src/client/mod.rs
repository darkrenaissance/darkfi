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

//! This module implements the client-side API for deploying arbitrary contracts
//! on the DarkFi network.

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{ContractId, Keypair, PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// `Deployooor::DeployV1` API
pub mod deploy_v1;

/// `Deployooor::LockV1` API
pub mod lock_v1;

pub struct DeriveContractIdRevealed {
    pub public_key: PublicKey,
    pub contract_id: ContractId,
}

impl DeriveContractIdRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let (pub_x, pub_y) = self.public_key.xy();
        vec![pub_x, pub_y, self.contract_id.inner()]
    }
}

pub fn create_derive_contractid_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    deploy_key: &Keypair,
) -> Result<(Proof, DeriveContractIdRevealed)> {
    let contract_id = ContractId::derive(deploy_key.secret);
    let public_inputs = DeriveContractIdRevealed { public_key: deploy_key.public, contract_id };
    let prover_witnesses = vec![Witness::Base(Value::known(deploy_key.secret.inner()))];
    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
