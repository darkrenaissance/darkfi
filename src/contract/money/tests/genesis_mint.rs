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

        // Holders this test will use
        const HOLDERS: [Holder; 2] = [Holder::Alice, Holder::Bob];

        // Some numbers we want to assert
        const ALICE_INITIAL: [u64; 1] = [100];
        const BOB_INITIAL: [u64; 2] = [100, 100];

        // Block height to verify against
        let current_block_height = 0;

        // Initialize harness
        let mut th = TestHarness::new(&HOLDERS, false).await?;

        info!(target: "money", "[Alice] ========================");
        info!(target: "money", "[Alice] Building genesis mint tx");
        info!(target: "money", "[Alice] ========================");
        let (genesis_mint_tx, genesis_mint_params) =
            th.genesis_mint(&Holder::Alice, &ALICE_INITIAL, None, None).await?;

        info!(target: "money", "[Malicious] =============================================");
        info!(target: "money", "[Malicious] Checking genesis mint tx not on genesis block");
        info!(target: "money", "[Malicious] =============================================");
        assert!(th
            .execute_genesis_mint_tx(
                &Holder::Alice,
                genesis_mint_tx.clone(),
                &genesis_mint_params,
                current_block_height + 1,
                true,
            )
            .await
            .is_err());

        for holder in &HOLDERS {
            info!(target: "money", "[{holder:?}] ================================");
            info!(target: "money", "[{holder:?}] Executing Alice genesis mint tx");
            info!(target: "money", "[{holder:?}] ================================");
            th.execute_genesis_mint_tx(
                holder,
                genesis_mint_tx.clone(),
                &genesis_mint_params,
                current_block_height,
                true,
            )
            .await?;
        }

        th.assert_trees(&HOLDERS);

        info!(target: "money", "[Bob] ========================");
        info!(target: "money", "[Bob] Building genesis mint tx");
        info!(target: "money", "[Bob] ========================");
        let (genesis_mint_tx, genesis_mint_params) =
            th.genesis_mint(&Holder::Bob, &BOB_INITIAL, None, None).await?;

        for holder in &HOLDERS {
            info!(target: "money", "[{holder:?}] =============================");
            info!(target: "money", "[{holder:?}] Executing Bob genesis mint tx");
            info!(target: "money", "[{holder:?}] =============================");
            th.execute_genesis_mint_tx(
                holder,
                genesis_mint_tx.clone(),
                &genesis_mint_params,
                current_block_height,
                true,
            )
            .await?;
        }

        th.assert_trees(&HOLDERS);

        let alice_owncoins = &th.holders.get(&Holder::Alice).unwrap().unspent_money_coins;
        let bob_owncoins = &th.holders.get(&Holder::Bob).unwrap().unspent_money_coins;
        assert!(alice_owncoins.len() == 1);
        assert!(alice_owncoins[0].note.value == ALICE_INITIAL[0]);
        assert!(bob_owncoins.len() == 2);
        assert!(bob_owncoins[0].note.value == BOB_INITIAL[0]);
        assert!(bob_owncoins[1].note.value == BOB_INITIAL[1]);

        // Thanks for reading
        Ok(())
    })
}
