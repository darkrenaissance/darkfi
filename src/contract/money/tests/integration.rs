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

//! Integration test for functionalities of the money smart contract:
//!
//! * Airdrops of the native token from the faucet
//! * Arbitrary token minting
//! * Transfers/Payments
//! * Atomic swaps
//! * Token mint freezing
//!
//! With this test we want to confirm the money contract state transitions
//! work between multiple parties and are able to be verified.
//! Note: Transfers and atomic swaps are covered in the mint_pay_swap test.
//!
//! TODO: Malicious cases

use darkfi::Result;
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness};
use log::info;

#[async_std::test]
async fn money_integration() -> Result<()> {
    init_logger();

    // Holders this test will use
    const HOLDERS: [Holder; 3] = [Holder::Faucet, Holder::Alice, Holder::Bob];

    // Some numbers we want to assert
    const ALICE_NATIVE_AIRDROP: u64 = 10000000000; // 100 DRK
    const BOB_SUPPLY: u64 = 2000000000; // 10 BOB

    // Slot to verify against
    let current_slot = 0;

    // Initialize harness
    let mut th = TestHarness::new(&["money".to_string()]).await?;

    info!("[Faucet] Building Alice airdrop tx");
    let (airdrop_tx, airdrop_params) = th.airdrop_native(ALICE_NATIVE_AIRDROP, Holder::Alice)?;

    info!("[Faucet] Executing Alice airdrop tx");
    th.execute_airdrop_native_tx(Holder::Faucet, &airdrop_tx, &airdrop_params, current_slot)
        .await?;

    info!("[Alice] Executing Alice airdrop tx");
    th.execute_airdrop_native_tx(Holder::Alice, &airdrop_tx, &airdrop_params, current_slot).await?;

    info!("[Bob] Executing Alice airdrop tx");
    th.execute_airdrop_native_tx(Holder::Bob, &airdrop_tx, &airdrop_params, current_slot).await?;

    th.assert_trees(&HOLDERS);

    // Alice gathers her new coin
    th.gather_owncoin(Holder::Alice, airdrop_params.outputs[0].clone(), None)?;

    info!("[Bob] Building BOB token mint tx");
    let (token_mint_tx, token_mint_params) = th.token_mint(BOB_SUPPLY, Holder::Bob, Holder::Bob)?;

    info!("[Faucet] Executing BOB token mint tx");
    th.execute_token_mint_tx(Holder::Faucet, &token_mint_tx, &token_mint_params, current_slot)
        .await?;

    info!("[Alice] Executing BOB token mint tx");
    th.execute_token_mint_tx(Holder::Alice, &token_mint_tx, &token_mint_params, current_slot)
        .await?;

    info!("[Bob] Executing BOB token mint tx");
    th.execute_token_mint_tx(Holder::Bob, &token_mint_tx, &token_mint_params, current_slot).await?;

    th.assert_trees(&HOLDERS);

    // Bob gathers his new coin
    th.gather_owncoin(Holder::Bob, token_mint_params.output, None)?;

    info!("[Bob] Building BOB token freeze tx");
    let (token_frz_tx, token_frz_params) = th.token_freeze(Holder::Bob)?;

    info!("[Faucet] Executing BOB token freeze tx");
    th.execute_token_freeze_tx(Holder::Faucet, &token_frz_tx, &token_frz_params, current_slot)
        .await?;

    info!("[Alice] Executing BOB token freeze tx");
    th.execute_token_freeze_tx(Holder::Alice, &token_frz_tx, &token_frz_params, current_slot)
        .await?;

    info!("[Bob] Executing BOB token freeze tx");
    th.execute_token_freeze_tx(Holder::Bob, &token_frz_tx, &token_frz_params, current_slot).await?;

    th.assert_trees(&HOLDERS);

    // Statistics
    th.statistics();

    // Thanks for reading
    Ok(())
}
