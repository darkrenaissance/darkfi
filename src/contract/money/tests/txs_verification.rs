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

//! Test for transaction verification correctness between Alice and Bob.
//!
//! We first mint Alice some tokens, and then she send some to Bob
//! a couple of times, including some double spending transactions.
//!
//! With this test, we want to confirm the transactions execution works
//! between multiple parties, with detection of erroneous transactions.

use darkfi::Result;
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness, TxAction};
use log::info;

#[test]
fn txs_verification() -> Result<()> {
    smol::block_on(async {
        init_logger();

        // Holders this test will use
        const HOLDERS: [Holder; 3] = [Holder::Faucet, Holder::Alice, Holder::Bob];

        // Some numbers we want to assert
        const ALICE_INITIAL: u64 = 100;

        // Alice = 50 ALICE
        // Bob = 50 ALICE
        const ALICE_SEND: u64 = ALICE_INITIAL - 50;

        // Slot to verify against
        let current_slot = 0;

        // Initialize harness
        let mut th = TestHarness::new(&["money".to_string()], false).await?;

        let mut alice_owncoins = vec![];
        let mut bob_owncoins = vec![];

        info!(target: "money", "[Alice] ================================");
        info!(target: "money", "[Alice] Building token mint tx for Alice");
        info!(target: "money", "[Alice] ================================");
        let (token_mint_tx, token_mint_params) =
            th.token_mint(ALICE_INITIAL, &Holder::Alice, &Holder::Alice, None, None)?;

        for holder in &HOLDERS {
            info!(target: "money", "[{holder:?}] =============================");
            info!(target: "money", "[{holder:?}] Executing Alice token mint tx");
            info!(target: "money", "[{holder:?}] =============================");
            th.execute_token_mint_tx(holder, &token_mint_tx, &token_mint_params, current_slot)
                .await?;
        }

        th.assert_trees(&HOLDERS);

        // Alice gathers her new owncoin
        let alice_oc = th.gather_owncoin(&Holder::Alice, &token_mint_params.output, None)?;
        let alice_token_id = alice_oc.note.token_id;
        alice_owncoins.push(alice_oc);

        // Now Alice can send a little bit of funds to Bob.
        // We can duplicate this transaction to simulate double spending.
        let duplicates = 3; // Change this number to >1 to double spend
        let mut transactions = vec![];
        let mut txs_params = vec![];
        for i in 0..duplicates {
            info!(target: "money", "[Alice] ======================================================");
            info!(target: "money", "[Alice] Building Money::Transfer params for payment {i} to Bob");
            info!(target: "money", "[Alice] ======================================================");
            let (transfer_tx, transfer_params, spent_coins) = th.transfer(
                ALICE_SEND,
                &Holder::Alice,
                &Holder::Bob,
                &alice_owncoins,
                alice_token_id,
            )?;

            // Validating transfer params
            assert!(transfer_params.inputs.len() == 1);
            assert!(transfer_params.outputs.len() == 2);
            assert!(spent_coins.len() == 1);

            // Now we simulate nodes verification, as transactions come one by one.
            // Validation should pass, even when we are trying to double spent.
            for holder in &HOLDERS {
                info!(target: "money", "[{holder:?}] ==================================");
                info!(target: "money", "[{holder:?}] Verifying Alice2Bob payment tx {i}");
                info!(target: "money", "[{holder:?}] ==================================");
                th.verify_transfer_tx(holder, &transfer_tx, current_slot).await?;
            }

            transactions.push(transfer_tx);
            txs_params.push(transfer_params);
        }
        alice_owncoins = vec![];
        assert_eq!(transactions.len(), duplicates);
        assert_eq!(txs_params.len(), duplicates);

        // Now we can try to execute the transactions sequentialy.
        // Each node will detect the duplicate txs and filter them out,
        // then only apply the first txs from the set.
        for holder in &HOLDERS {
            info!(target: "money", "[{holder:?}] ==============================");
            info!(target: "money", "[{holder:?}] Executing Alice2Bob payment tx");
            info!(target: "money", "[{holder:?}] ==============================");
            th.execute_erroneous_txs(
                TxAction::MoneyTransfer,
                holder,
                &transactions,
                current_slot,
                duplicates - 1,
            )
            .await?;
            th.execute_transfer_tx(holder, &transactions[0], &txs_params[0], current_slot, true)
                .await?;
        }

        th.assert_trees(&HOLDERS);

        // Bob should now have the new OwnCoin.
        let bob_oc = th.gather_owncoin(&Holder::Bob, &txs_params[0].outputs[0], None)?;
        bob_owncoins.push(bob_oc);

        // Alice should now have one OwnCoin with the change from the above transaction.
        let alice_oc = th.gather_owncoin(&Holder::Alice, &txs_params[0].outputs[1], None)?;
        alice_owncoins.push(alice_oc);

        assert!(alice_owncoins.len() == 1);
        assert!(bob_owncoins.len() == 1);

        // Statistics
        th.statistics();

        // Thanks for reading
        Ok(())
    })
}
