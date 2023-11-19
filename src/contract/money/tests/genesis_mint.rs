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

//! Test for genesis transaction verification correctness between Alice and Bob.
//!
//! We first mint Alice some native tokens on genesis slot, and then she send
//! some of them to Bob.
//!
//! With this test, we want to confirm the genesis transactions execution works
//! and generated tokens can be processed as usual between multiple parties,
//! with detection of erroneous transactions.

use darkfi::Result;
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness, TxAction};
use darkfi_sdk::crypto::DARK_TOKEN_ID;
use log::info;

#[test]
fn genesis_mint() -> Result<()> {
    smol::block_on(async {
        init_logger();

        // Holders this test will use
        const HOLDERS: [Holder; 3] = [Holder::Faucet, Holder::Alice, Holder::Bob];

        // Some numbers we want to assert
        const ALICE_INITIAL: u64 = 100;
        const BOB_INITIAL: u64 = 200;

        // Alice = 50 DARK
        // Bob = 250 DARK
        const ALICE_SEND: u64 = ALICE_INITIAL - 50;
        // Alice = 230 DARK
        // Bob = 50
        const BOB_SEND: u64 = BOB_INITIAL - 20;

        // Slot to verify against
        let current_slot = 0;

        // Initialize harness
        let mut th = TestHarness::new(&["money".to_string()]).await?;

        let mut alice_owncoins = vec![];
        let mut bob_owncoins = vec![];

        info!(target: "money", "[Alice] ========================");
        info!(target: "money", "[Alice] Building genesis mint tx");
        info!(target: "money", "[Alice] ========================");
        let (genesis_mint_tx, genesis_mint_params) =
            th.genesis_mint(&Holder::Alice, ALICE_INITIAL)?;

        // We are going to use alice genesis mint transaction to
        // test some malicious cases.
        info!(target: "money", "[Malicious] ==================================");
        info!(target: "money", "[Malicious] Checking duplicate genesis mint tx");
        info!(target: "money", "[Malicious] ==================================");
        th.execute_erroneous_txs(
            TxAction::MoneyGenesisMint,
            &Holder::Alice,
            &[genesis_mint_tx.clone(), genesis_mint_tx.clone()],
            current_slot,
            1,
        )
        .await?;

        info!(target: "money", "[Malicious] ============================================");
        info!(target: "money", "[Malicious] Checking genesis mint tx not on genesis slot");
        info!(target: "money", "[Malicious] ============================================");
        th.execute_erroneous_txs(
            TxAction::MoneyGenesisMint,
            &Holder::Alice,
            &[genesis_mint_tx.clone()],
            current_slot + 1,
            1,
        )
        .await?;

        for holder in &HOLDERS {
            info!(target: "money", "[{holder:?}] ================================");
            info!(target: "money", "[{holder:?}] Executing Alice genesis mint tx");
            info!(target: "money", "[{holder:?}] ================================");
            th.execute_genesis_mint_tx(
                holder,
                &genesis_mint_tx,
                &genesis_mint_params,
                current_slot,
            )
            .await?;
        }

        th.assert_trees(&HOLDERS);

        // Alice gathers her new owncoin
        let alice_oc = th.gather_owncoin(&Holder::Alice, &genesis_mint_params.output, None)?;
        alice_owncoins.push(alice_oc);

        info!(target: "money", "[Bob] ========================");
        info!(target: "money", "[Bob] Building genesis mint tx");
        info!(target: "money", "[Bob] ========================");
        let (genesis_mint_tx, genesis_mint_params) = th.genesis_mint(&Holder::Bob, BOB_INITIAL)?;

        for holder in &HOLDERS {
            info!(target: "money", "[{holder:?}] =============================");
            info!(target: "money", "[{holder:?}] Executing Bob genesis mint tx");
            info!(target: "money", "[{holder:?}] =============================");
            th.execute_genesis_mint_tx(
                holder,
                &genesis_mint_tx,
                &genesis_mint_params,
                current_slot,
            )
            .await?;
        }

        th.assert_trees(&HOLDERS);

        // Bob gathers his new owncoin
        let bob_oc = th.gather_owncoin(&Holder::Bob, &genesis_mint_params.output, None)?;
        bob_owncoins.push(bob_oc);

        // Now Alice can send a little bit of funds to Bob
        info!(target: "money", "[Alice] ====================================================");
        info!(target: "money", "[Alice] Building Money::Transfer params for a payment to Bob");
        info!(target: "money", "[Alice] ====================================================");
        let (transfer_tx, transfer_params, spent_coins) =
            th.transfer(ALICE_SEND, &Holder::Alice, &Holder::Bob, &alice_owncoins, *DARK_TOKEN_ID)?;

        // Validating transfer params
        assert!(transfer_params.inputs.len() == 1);
        assert!(transfer_params.outputs.len() == 2);
        assert!(spent_coins.len() == 1);
        alice_owncoins.retain(|x| x != &spent_coins[0]);
        assert!(alice_owncoins.is_empty());

        for holder in &HOLDERS {
            info!(target: "money", "[{holder:?}] ==============================");
            info!(target: "money", "[{holder:?}] Executing Alice2Bob payment tx");
            info!(target: "money", "[{holder:?}] ==============================");
            th.execute_transfer_tx(holder, &transfer_tx, &transfer_params, current_slot, true)
                .await?;
        }

        th.assert_trees(&HOLDERS);

        // Bob should have his old OwnCoin, and this new one.
        let bob_oc = th.gather_owncoin(&Holder::Bob, &transfer_params.outputs[0], None)?;
        bob_owncoins.push(bob_oc);

        // Alice should now have one OwnCoin with the change from the above transaction.
        let alice_oc = th.gather_owncoin(&Holder::Alice, &transfer_params.outputs[1], None)?;
        alice_owncoins.push(alice_oc);

        assert!(alice_owncoins.len() == 1);
        assert!(bob_owncoins.len() == 2);

        // Bob can send a little bit to Alice as well
        info!(target: "money", "[Bob] ======================================================");
        info!(target: "money", "[Bob] Building Money::Transfer params for a payment to Alice");
        info!(target: "money", "[Bob] ======================================================");
        let (transfer_tx, transfer_params, spent_coins) =
            th.transfer(BOB_SEND, &Holder::Bob, &Holder::Alice, &bob_owncoins, *DARK_TOKEN_ID)?;

        // Validating transfer params
        assert!(transfer_params.inputs.len() == 1);
        assert!(transfer_params.outputs.len() == 2);
        assert!(spent_coins.len() == 1);
        bob_owncoins.retain(|x| x != &spent_coins[0]);
        assert!(bob_owncoins.len() == 1);

        for holder in &HOLDERS {
            info!(target: "money", "[{holder:?}] ==============================");
            info!(target: "money", "[{holder:?}] Executing Bob2Alice payment tx");
            info!(target: "money", "[{holder:?}] ==============================");
            th.execute_transfer_tx(holder, &transfer_tx, &transfer_params, current_slot, true)
                .await?;
        }

        th.assert_trees(&HOLDERS);

        // Alice should now have two OwnCoins
        let alice_oc = th.gather_owncoin(&Holder::Alice, &transfer_params.outputs[0], None)?;
        alice_owncoins.push(alice_oc);

        // Bob should have two with the change from the above tx
        let bob_oc = th.gather_owncoin(&Holder::Bob, &transfer_params.outputs[1], None)?;
        bob_owncoins.push(bob_oc);

        // Validating transaction outcomes
        assert!(alice_owncoins.len() == 2);
        assert!(bob_owncoins.len() == 2);
        assert!(alice_owncoins[0].note.value == ALICE_INITIAL - ALICE_SEND);
        assert!(alice_owncoins[1].note.value == BOB_SEND);
        assert!(bob_owncoins[0].note.value == ALICE_SEND);
        assert!(bob_owncoins[1].note.value == BOB_INITIAL - BOB_SEND);

        // Statistics
        th.statistics();

        // Thanks for reading
        Ok(())
    })
}
