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

use darkfi::Result;
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness};
use darkfi_sdk::crypto::BaseBlind;
use rand::rngs::OsRng;
use tracing::info;

#[test]
fn token_mint() -> Result<()> {
    smol::block_on(async {
        init_logger();

        // Holders this test will use
        const HOLDERS: [Holder; 2] = [Holder::Alice, Holder::Bob];

        // Some numbers we want to assert
        const BOB_SUPPLY: u64 = 2000000000; // 10 BOB

        // Block height to verify against
        let current_block_height = 0;

        // Initialize harness
        let mut th = TestHarness::new(&HOLDERS, false).await?;

        info!("[Bob] Building BOB token mint tx");
        let bob_token_blind = BaseBlind::random(&mut OsRng);
        let (token_mint_tx, token_mint_params, token_auth_mint_params, fee_params) = th
            .token_mint(
                BOB_SUPPLY,
                &Holder::Bob,
                &Holder::Bob,
                bob_token_blind,
                None,
                None,
                current_block_height,
            )
            .await?;

        for holder in &HOLDERS {
            info!("[{holder:?}] Executing BOB token mint tx");
            th.execute_token_mint_tx(
                holder,
                token_mint_tx.clone(),
                &token_mint_params,
                &token_auth_mint_params,
                &fee_params,
                current_block_height,
                true,
            )
            .await?;
        }

        th.assert_trees(&HOLDERS);

        info!("[Bob] Building BOB token freeze tx");
        let (token_frz_tx, token_frz_params, fee_params) =
            th.token_freeze(&Holder::Bob, current_block_height).await?;

        for holder in &HOLDERS {
            info!("[{holder:?}] Executing BOB token freeze tx");
            th.execute_token_freeze_tx(
                holder,
                token_frz_tx.clone(),
                &token_frz_params,
                &fee_params,
                current_block_height,
                true,
            )
            .await?;
        }

        th.assert_trees(&HOLDERS);

        // Thanks for reading
        Ok(())
    })
}
