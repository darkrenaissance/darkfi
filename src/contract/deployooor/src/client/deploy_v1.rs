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

use darkfi::Result;
use darkfi_sdk::{crypto::Keypair, deploy::DeployParamsV1};
use log::info;

pub struct DeployCallDebris {
    pub params: DeployParamsV1,
}

/// Struct holding necessary information to build a `Deployooor::DeployV1` contract call.
pub struct DeployCallBuilder {
    /// Contract deploy keypair
    pub deploy_keypair: Keypair,
    /// WASM bincode to deploy
    pub wasm_bincode: Vec<u8>,
    /// Serialized deployment payload instruction
    pub deploy_ix: Vec<u8>,
}

impl DeployCallBuilder {
    pub fn build(&self) -> Result<DeployCallDebris> {
        info!("Building Deployooor::DeployV1 contract call");
        assert!(!self.wasm_bincode.is_empty());

        let params = DeployParamsV1 {
            wasm_bincode: self.wasm_bincode.clone(),
            public_key: self.deploy_keypair.public,
            ix: self.deploy_ix.clone(),
        };

        let debris = DeployCallDebris { params };

        Ok(debris)
    }
}
