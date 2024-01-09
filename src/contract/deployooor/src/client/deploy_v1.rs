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
use darkfi_sdk::crypto::Keypair;
use log::{debug, info};

use super::create_derive_contractid_proof;
use crate::model::DeployParamsV1;

pub struct DeployCallDebris {
    pub params: DeployParamsV1,
    pub proofs: Vec<Proof>,
}

/// Struct holding necessary information to build a `Deployooor::DeployV1` contract call.
pub struct DeployCallBuilder {
    /// Contract deploy keypair
    pub deploy_keypair: Keypair,
    /// WASM bincode to deploy
    pub wasm_bincode: Vec<u8>,
    /// Serialized deployment payload instruction
    pub deploy_ix: Vec<u8>,
    /// `DeriveContractID` zkas circuit ZkBinary
    pub derivecid_zkbin: ZkBinary,
    /// Proving key for the `DeriveContractID` zk circuit
    pub derivecid_pk: ProvingKey,
}

impl DeployCallBuilder {
    pub fn build(&self) -> Result<DeployCallDebris> {
        info!("Building Deployooor::DeployV1 contract call");
        assert!(!self.wasm_bincode.is_empty());

        debug!("Creating DeriveContractID ZK proof");
        let (proof, _public_inputs) = create_derive_contractid_proof(
            &self.derivecid_zkbin,
            &self.derivecid_pk,
            &self.deploy_keypair,
        )?;

        let params = DeployParamsV1 {
            wasm_bincode: self.wasm_bincode.clone(),
            public_key: self.deploy_keypair.public,
            ix: self.deploy_ix.clone(),
        };

        let debris = DeployCallDebris { params, proofs: vec![proof] };

        Ok(debris)
    }
}
