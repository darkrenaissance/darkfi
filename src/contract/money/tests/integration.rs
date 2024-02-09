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
use darkfi_sdk::blockchain::expected_reward;
use log::info;

#[test]
fn money_integration() -> Result<()> {
    smol::block_on(async {
        init_logger();

        // Holders this test will use
        const HOLDERS: [Holder; 2] = [Holder::Alice, Holder::Bob];

        // Initialize harness
        let mut th = TestHarness::new(&HOLDERS, true).await?;

        // Block height to verify against
        let current_block_height = 1;

        // Drop some money to Alice
        info!("[Alice] Building block proposal");
        let (alice_proposal_tx, alice_proposal_params) =
            th.pow_reward(&Holder::Alice, None, None).await?;

        for holder in HOLDERS {
            info!("[{holder:?}] Executing Alice's block proposal");
            th.execute_pow_reward_tx(
                &holder,
                &alice_proposal_tx,
                &alice_proposal_params,
                current_block_height,
            )
            .await?;
        }

        let alice_owncoins = th.holders.get(&Holder::Alice).unwrap().unspent_money_coins.clone();
        assert!(alice_owncoins[0].note.value == expected_reward(current_block_height));

        th.assert_trees(&HOLDERS);

        // And some to Bob
        info!("[Bob] Building block proposal");
        let (bob_proposal_tx, bob_proposal_params) =
            th.pow_reward(&Holder::Bob, None, None).await?;

        for holder in HOLDERS {
            info!("[{holder:?}] Executing Alice's block proposal");
            th.execute_pow_reward_tx(
                &holder,
                &bob_proposal_tx,
                &bob_proposal_params,
                current_block_height,
            )
            .await?;
        }

        th.assert_trees(&HOLDERS);

        // Alice sends a payment of some DRK to Bob.

        // Thanks for reading
        Ok(())
    })
}
