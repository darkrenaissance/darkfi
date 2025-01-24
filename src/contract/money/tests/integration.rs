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
use darkfi_sdk::blockchain::expected_reward;

#[test]
fn money_integration() -> Result<()> {
    smol::block_on(async {
        init_logger();

        // Holders this test will use
        const HOLDERS: [Holder; 2] = [Holder::Alice, Holder::Bob];

        // Initialize harness
        let mut th = TestHarness::new(&HOLDERS, true).await?;

        // Generate two new blocks mined by Alice
        th.generate_block(&Holder::Alice, &HOLDERS).await?;
        th.generate_block(&Holder::Alice, &HOLDERS).await?;

        // Generate two new blocks mined by Bob
        th.generate_block(&Holder::Bob, &HOLDERS).await?;
        th.generate_block(&Holder::Bob, &HOLDERS).await?;

        // Assert correct rewards
        let alice_coins = &th.holders.get(&Holder::Alice).unwrap().unspent_money_coins;
        let bob_coins = &th.holders.get(&Holder::Bob).unwrap().unspent_money_coins;
        assert!(alice_coins.len() == 2);
        assert!(bob_coins.len() == 2);
        assert!(alice_coins[0].note.value == expected_reward(1));
        assert!(alice_coins[1].note.value == expected_reward(2));
        assert!(bob_coins[0].note.value == expected_reward(3));
        assert!(bob_coins[1].note.value == expected_reward(4));

        let current_block_height = 4;

        // Alice transfers some tokens to Bob
        let (tx, (xfer_params, fee_params), _spent_soins) = th
            .transfer(
                alice_coins[0].note.value,
                &Holder::Alice,
                &Holder::Bob,
                &[alice_coins[0].clone()],
                alice_coins[0].note.token_id,
                current_block_height,
                false,
            )
            .await?;

        // Execute the transaction
        for holder in &HOLDERS {
            th.execute_transfer_tx(
                holder,
                tx.clone(),
                &xfer_params,
                &fee_params,
                current_block_height,
                true,
            )
            .await?;
        }

        // Assert coins in wallets
        let alice_coins = &th.holders.get(&Holder::Alice).unwrap().unspent_money_coins;
        let bob_coins = &th.holders.get(&Holder::Bob).unwrap().unspent_money_coins;
        assert!(alice_coins.len() == 1); // Change from fee
        assert!(bob_coins.len() == 3);
        assert!(bob_coins[2].note.value == expected_reward(1));

        // Thanks for reading
        Ok(())
    })
}
