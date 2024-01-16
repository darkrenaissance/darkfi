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
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness};
use log::info;

#[test]
fn token_mint() -> Result<()> {
    smol::block_on(async {
        init_logger();

        // Holders this test will use
        const HOLDERS: [Holder; 2] = [Holder::Alice, Holder::Bob];

        // Some numbers we want to assert
        const BOB_SUPPLY: u64 = 2000000000; // 10 BOB

        // Slot to verify against
        let current_slot = 0;

        // Initialize harness
        let mut th = TestHarness::new(&["money".to_string()], false).await?;

        info!("[Bob] Building BOB token mint tx");
        let (token_mint_tx, token_mint_params) =
            th.token_mint(BOB_SUPPLY, &Holder::Bob, &Holder::Bob, None, None)?;

        for holder in &HOLDERS {
            info!("[{holder:?}] Executing BOB token mint tx");
            th.execute_token_mint_tx(holder, &token_mint_tx, &token_mint_params, current_slot)
                .await?;
        }

        th.assert_trees(&HOLDERS);

        // Bob gathers his new coin
        th.gather_owncoin(&Holder::Bob, &token_mint_params.output, None)?;

        info!("[Bob] Building BOB token freeze tx");
        let (token_frz_tx, token_frz_params) = th.token_freeze(&Holder::Bob)?;

        for holder in &HOLDERS {
            info!("[{holder:?}] Executing BOB token freeze tx");
            th.execute_token_freeze_tx(holder, &token_frz_tx, &token_frz_params, current_slot)
                .await?;
        }

        th.assert_trees(&HOLDERS);

        // Statistics
        th.statistics();

        // Thanks for reading
        Ok(())
    })
}
