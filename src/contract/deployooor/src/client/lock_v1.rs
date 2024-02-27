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
use darkfi_sdk::crypto::Keypair;
use log::info;

use crate::model::LockParamsV1;

pub struct LockCallDebris {
    pub params: LockParamsV1,
}

/// Struct holding necessary information to build a `Deployooor::LockV1` contract call.
pub struct LockCallBuilder {
    /// Contract deploy keypair
    pub deploy_keypair: Keypair,
}

impl LockCallBuilder {
    pub fn build(&self) -> Result<LockCallDebris> {
        info!("Building Deployooor::LockV1 contract call");

        let params = LockParamsV1 { public_key: self.deploy_keypair.public };
        let debris = LockCallDebris { params };

        Ok(debris)
    }
}
