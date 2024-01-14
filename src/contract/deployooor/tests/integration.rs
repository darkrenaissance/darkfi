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

use darkfi::Result;
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness};
use log::info;

#[test]
fn deploy_integration() -> Result<()> {
    smol::block_on(async {
        init_logger();

        // Slot to verify against
        let current_slot = 0;

        // Initialize harness
        let mut th =
            TestHarness::new(&["money".to_string(), "deployooor".to_string()], false).await?;

        // WASM bincode to deploy
        let wasm_bincode = include_bytes!("../../dao/darkfi_dao_contract.wasm");

        info!("[Alice] Building deploy tx");
        let (deploy_tx, deploy_params) =
            th.deploy_contract(&Holder::Alice, wasm_bincode.to_vec())?;

        info!("[Alice] Executing deploy tx");
        th.execute_deploy_tx(&Holder::Alice, &deploy_tx, &deploy_params, current_slot).await?;

        // Thanks for reading
        Ok(())
    })
}
