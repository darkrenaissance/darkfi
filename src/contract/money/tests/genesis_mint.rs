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

//! Test for genesis transaction verification correctness between Alice and Bob.
//!
//! We first mint Alice some native tokens on genesis block, and then she send
//! some of them to Bob.
//!
//! With this test, we want to confirm the genesis transactions execution works
//! and generated tokens can be processed as usual between multiple parties,
//! with detection of erroneous transactions.

use darkfi::Result;
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness};
use tracing::info;

#[test]
fn genesis_mint() -> Result<()> {
    smol::block_on(async {
        init_logger();

        init_logger();

        use Holder::{Alice, Bob};

        const ALICE_INITIAL: u64 = 100;
        const BOB_AMOUNTS: [u64; 2] = [100, 100];

        let block_height = 0;

        let mut th = TestHarness::new(&[Alice, Bob], false).await?;

        // Build Alice's genesis mint
        info!(target: "money", "Building Alice genesis mint tx");
        let (genesis_tx, genesis_params) =
            th.genesis_mint(&Alice, &[ALICE_INITIAL], None, None).await?;

        // Malicious: verify genesis mint fails on non-genesis block
        info!(target: "money", "Checking genesis mint tx not on genesis block");
        assert!(th
            .execute_genesis_mint_tx(
                &Alice,
                genesis_tx.clone(),
                &genesis_params,
                block_height + 1,
                true,
            )
            .await
            .is_err());

        // Execute on all holders
        info!(target: "money", "Executing Alice genesis mint tx on all holders");
        th.genesis_mint_to_all_with(genesis_tx, &genesis_params, block_height).await?;

        // Build and execute Bob's genesis mint
        info!(target: "money", "Building and executing Bob genesis mint tx");
        let (genesis_tx, genesis_params) = th.genesis_mint(&Bob, &BOB_AMOUNTS, None, None).await?;
        th.genesis_mint_to_all_with(genesis_tx, &genesis_params, block_height).await?;

        // Assert final state
        assert_eq!(th.coins(&Alice).len(), 1);
        assert_eq!(th.coins(&Alice)[0].note.value, ALICE_INITIAL);
        assert_eq!(th.coins(&Bob).len(), 2);
        assert_eq!(th.coins(&Bob)[0].note.value, BOB_AMOUNTS[0]);
        assert_eq!(th.coins(&Bob)[1].note.value, BOB_AMOUNTS[1]);

        // Thanks for reading
        Ok(())
    })
}
