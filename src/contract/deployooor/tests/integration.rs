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

use darkfi::Result;
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness};
use tracing::info;

#[test]
fn deploy_integration() -> Result<()> {
    smol::block_on(async {
        init_logger();

        // Block height to verify against
        let current_block_height = 0;

        // Initialize harness
        let mut th = TestHarness::new(&[Holder::Alice], false).await?;

        // WASM bincode to deploy
        let wasm_bincode = include_bytes!("../../dao/darkfi_dao_contract.wasm");

        info!(target: "deploy", "[Alice] Building deploy tx");
        let (deploy_tx, deploy_params, fee_params) =
            th.deploy_contract(&Holder::Alice, wasm_bincode.to_vec(), current_block_height).await?;

        info!(target: "deploy", "[Alice] Executing deploy tx");
        th.execute_deploy_tx(
            &Holder::Alice,
            deploy_tx,
            &deploy_params,
            &fee_params,
            current_block_height,
            true,
        )
        .await?;

        // WASM bincode to deploy as a replacement
        let wasm_bincode = include_bytes!("../../money/darkfi_money_contract.wasm");

        info!(target: "deploy", "[Alice] Building deploy replacement tx");
        let (deploy_tx, deploy_params, fee_params) =
            th.deploy_contract(&Holder::Alice, wasm_bincode.to_vec(), current_block_height).await?;

        info!(target: "deploy", "[Alice] Executing deploy replacement tx");
        th.execute_deploy_tx(
            &Holder::Alice,
            deploy_tx,
            &deploy_params,
            &fee_params,
            current_block_height,
            true,
        )
        .await?;

        info!(target: "deploy", "[Alice] Building deploy lock tx");
        let (lock_tx, lock_params, fee_params) =
            th.lock_contract(&Holder::Alice, current_block_height).await?;

        info!(target: "deploy", "[Alice] Executing deploy lock tx");
        th.execute_lock_tx(
            &Holder::Alice,
            lock_tx,
            &lock_params,
            &fee_params,
            current_block_height,
            true,
        )
        .await?;

        info!(target: "deploy", "[Malicious] ===============================");
        info!(target: "deploy", "[Malicious] Checking locking contract again");
        info!(target: "deploy", "[Malicious] ===============================");
        let (lock_tx, lock_params, fee_params) =
            th.lock_contract(&Holder::Alice, current_block_height).await?;
        assert!(th
            .execute_lock_tx(
                &Holder::Alice,
                lock_tx,
                &lock_params,
                &fee_params,
                current_block_height,
                true,
            )
            .await
            .is_err());

        info!(target: "deploy", "[Malicious] ====================================");
        info!(target: "deploy", "[Malicious] Checking deploy on a locked contract");
        info!(target: "deploy", "[Malicious] ====================================");
        let (deploy_tx, deploy_params, fee_params) =
            th.deploy_contract(&Holder::Alice, wasm_bincode.to_vec(), current_block_height).await?;
        assert!(th
            .execute_deploy_tx(
                &Holder::Alice,
                deploy_tx,
                &deploy_params,
                &fee_params,
                current_block_height,
                true,
            )
            .await
            .is_err());

        // Thanks for reading
        Ok(())
    })
}
