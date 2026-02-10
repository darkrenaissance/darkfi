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

//! Integration test for the Deployooor contract.
//!
//! Tests the deploy/lock lifecycle with a single holder:
//!   1. Deploy a WASM contract (DAO)
//!   2. Replace the deployed contract with different WASM (Money)
//!   3. Lock the contract to prevent further changes
//!   4. Negative: locking an already-locked contract fails
//!   5. Negative: deploying to a locked contract fails

use darkfi::Result;
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness};
use tracing::info;

#[test]
fn deploy_integration() -> Result<()> {
    smol::block_on(async {
        init_logger();

        use Holder::Alice;

        let block_height = 0;
        let mut th = TestHarness::new(&[Alice], false).await?;

        let dao_wasm = include_bytes!("../../dao/darkfi_dao_contract.wasm");
        let money_wasm = include_bytes!("../../money/darkfi_money_contract.wasm");

        // Deploy a contract
        info!(target: "deploy", "Deploying DAO contract");
        let (tx, params, fee_params) =
            th.deploy_contract(&Alice, dao_wasm.to_vec(), block_height).await?;
        th.execute_deploy_tx(&Alice, tx, &params, &fee_params, block_height, true).await?;

        // Replace the deployed contract with different WASM
        info!(target: "deploy", "Replacing with Money contract");
        let (tx, params, fee_params) =
            th.deploy_contract(&Alice, money_wasm.to_vec(), block_height).await?;
        th.execute_deploy_tx(&Alice, tx, &params, &fee_params, block_height, true).await?;

        // Lock the contract
        info!(target: "deploy", "Locking contract");
        let (tx, params, fee_params) = th.lock_contract(&Alice, block_height).await?;
        th.execute_lock_tx(&Alice, tx, &params, &fee_params, block_height, true).await?;

        // Negative: locking an already-locked contract must fail
        info!(target: "deploy", "Verifying double-lock is rejected");
        let (tx, params, fee_params) = th.lock_contract(&Alice, block_height).await?;
        assert!(th
            .execute_lock_tx(&Alice, tx, &params, &fee_params, block_height, true)
            .await
            .is_err());

        // Negative: deploying to a locked contract must fail
        info!(target: "deploy", "Verifying deploy-after-lock is rejected");
        let (tx, params, fee_params) =
            th.deploy_contract(&Alice, money_wasm.to_vec(), block_height).await?;
        assert!(th
            .execute_deploy_tx(&Alice, tx, &params, &fee_params, block_height, true)
            .await
            .is_err());

        // Thanks for reading
        Ok(())
    })
}
