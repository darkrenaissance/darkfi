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
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::crypto::Keypair;
use rand::rngs::OsRng;
use wasm_hello_world::HelloParams;

pub struct ContractCallDebris {
    pub params: HelloParams,
    pub proofs: Vec<Proof>,
}

/// Struct holding necessary information to build a wasm-hello-world contract call.
pub struct ContractCallBuilder {
    /// Member keypair this call is for
    pub member: Keypair,
    /// `SecretCommitment` zkas circuit ZkBinary
    pub commitment_zkbin: ZkBinary,
    /// Proving key for the `SecretCommitment` zk circuit,
    pub commitment_pk: ProvingKey,
}

impl ContractCallBuilder {
    pub fn build(&self) -> Result<ContractCallDebris> {
        // Build the commitment proof
        let prover_witnesses = vec![Witness::Base(Value::known(self.member.secret.inner()))];
        let (public_x, public_y) = self.member.public.xy();
        let public_inputs = vec![public_x, public_y];
        let circuit = ZkCircuit::new(prover_witnesses, &self.commitment_zkbin);
        let proof = Proof::create(&self.commitment_pk, &[circuit], &public_inputs, &mut OsRng)?;

        // Generate the params and call debris
        let params = HelloParams { x: public_x, y: public_y };
        let debris = ContractCallDebris { params, proofs: vec![proof] };
        Ok(debris)
    }
}
