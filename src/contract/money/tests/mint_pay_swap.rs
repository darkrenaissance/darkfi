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

//! Integration test for payments between Alice and Bob.
//!
//! We first mint them different tokens, and then they send them to each
//! other a couple of times.
//!
//! With this test, we want to confirm the money contract transfer state
//! transitions work between multiple parties and are able to be verified.
//! We also test atomic swaps with some of the coins that have been produced.
//!
//! TODO: Malicious cases

use darkfi::Result;
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness};
use darkfi_sdk::crypto::BaseBlind;
use log::info;
use rand::rngs::OsRng;

#[test]
fn mint_pay_swap() -> Result<()> {
    smol::block_on(async {
        init_logger();

        // Holders this test will use
        const HOLDERS: [Holder; 2] = [Holder::Alice, Holder::Bob];

        // Some numbers we want to assert
        const ALICE_INITIAL: u64 = 100;
        const BOB_INITIAL: u64 = 200;

        // Alice = 50 ALICE
        // Bob = 200 BOB + 50 ALICE
        const ALICE_FIRST_SEND: u64 = ALICE_INITIAL - 50;
        // Alice = 50 ALICE + 180 BOB
        // Bob = 20 BOB + 50 ALICE
        const BOB_FIRST_SEND: u64 = BOB_INITIAL - 20;

        // Block height to verify against
        let current_block_height = 0;

        // Initialize harness
        let mut th = TestHarness::new(&HOLDERS, false).await?;

        info!(target: "money", "[Alice] ================================");
        info!(target: "money", "[Alice] Building token mint tx for Alice");
        info!(target: "money", "[Alice] ================================");
        let alice_token_blind = BaseBlind::random(&mut OsRng);
        let (mint_tx, mint_params, mint_auth_params, fee_params) = th
            .token_mint(
                ALICE_INITIAL,
                &Holder::Alice,
                &Holder::Alice,
                alice_token_blind,
                None,
                None,
                current_block_height,
            )
            .await?;

        for holder in &HOLDERS {
            info!(target: "money", "[{holder:?}] ==============================");
            info!(target: "money", "[{holder:?}] Executing Alice token mint tx");
            info!(target: "money", "[{holder:?}] ==============================");
            th.execute_token_mint_tx(
                holder,
                mint_tx.clone(),
                &mint_params,
                &mint_auth_params,
                &fee_params,
                current_block_height,
                true,
            )
            .await?;
        }

        th.assert_trees(&HOLDERS);

        info!(target: "money", "[Bob] ==============================");
        info!(target: "money", "[Bob] Building token mint tx for Bob");
        info!(target: "money", "[Bob] ==============================");
        let bob_token_blind = BaseBlind::random(&mut OsRng);
        let (mint_tx, mint_params, mint_auth_params, fee_params) = th
            .token_mint(
                BOB_INITIAL,
                &Holder::Bob,
                &Holder::Bob,
                bob_token_blind,
                None,
                None,
                current_block_height,
            )
            .await?;

        for holder in &HOLDERS {
            info!(target: "money", "[{holder:?}] ===========================");
            info!(target: "money", "[{holder:?}] Executing Bob token mint tx");
            info!(target: "money", "[{holder:?}] ===========================");
            th.execute_token_mint_tx(
                holder,
                mint_tx.clone(),
                &mint_params,
                &mint_auth_params,
                &fee_params,
                current_block_height,
                true,
            )
            .await?;
        }

        th.assert_trees(&HOLDERS);

        // Now Alice can send a little bit of funds to Bob
        info!(target: "money", "[Alice] ====================================================");
        info!(target: "money", "[Alice] Building Money::Transfer params for a payment to Bob");
        info!(target: "money", "[Alice] ====================================================");
        let mut alice_owncoins =
            th.holders.get(&Holder::Alice).unwrap().unspent_money_coins.clone();
        let alice_token_id = alice_owncoins[0].note.token_id;

        let (transfer_tx, (transfer_params, fee_params), spent_coins) = th
            .transfer(
                ALICE_FIRST_SEND,
                &Holder::Alice,
                &Holder::Bob,
                &alice_owncoins,
                alice_token_id,
                current_block_height,
                false,
            )
            .await?;

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
            th.execute_transfer_tx(
                holder,
                transfer_tx.clone(),
                &transfer_params,
                &fee_params,
                current_block_height,
                true,
            )
            .await?;
        }

        // Alice should now have one OwnCoin with the change from the above transfer.
        // Bob should now have a new OwnCoin.
        let alice_owncoins = &th.holders.get(&Holder::Alice).unwrap().unspent_money_coins;
        let mut bob_owncoins = th.holders.get(&Holder::Bob).unwrap().unspent_money_coins.clone();
        assert!(alice_owncoins.len() == 1);
        assert!(bob_owncoins.len() == 2);

        th.assert_trees(&HOLDERS);

        // Bob can send a little bit to Alice as well
        info!(target: "money", "[Bob] ======================================================");
        info!(target: "money", "[Bob] Building Money::Transfer params for a payment to Alice");
        info!(target: "money", "[Bob] ======================================================");
        let bob_token_id = bob_owncoins[0].note.token_id;
        let mut bob_owncoins_tmp = bob_owncoins.clone();
        bob_owncoins_tmp.retain(|x| x.note.token_id == bob_token_id);
        let (transfer_tx, (transfer_params, fee_params), spent_coins) = th
            .transfer(
                BOB_FIRST_SEND,
                &Holder::Bob,
                &Holder::Alice,
                &bob_owncoins_tmp,
                bob_token_id,
                current_block_height,
                false,
            )
            .await?;

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
            th.execute_transfer_tx(
                holder,
                transfer_tx.clone(),
                &transfer_params,
                &fee_params,
                current_block_height,
                true,
            )
            .await?;
        }

        // Alice should now have two OwnCoins
        // Bob should have two with the change from the above tx
        let mut alice_owncoins =
            th.holders.get(&Holder::Alice).unwrap().unspent_money_coins.clone();
        let mut bob_owncoins = th.holders.get(&Holder::Bob).unwrap().unspent_money_coins.clone();

        assert!(alice_owncoins.len() == 2);
        assert!(bob_owncoins.len() == 2);

        assert!(alice_owncoins[0].note.value == ALICE_INITIAL - ALICE_FIRST_SEND);
        assert!(alice_owncoins[0].note.token_id == alice_token_id);
        assert!(alice_owncoins[1].note.value == BOB_FIRST_SEND);
        assert!(alice_owncoins[1].note.token_id == bob_token_id);

        assert!(bob_owncoins[0].note.value == ALICE_FIRST_SEND);
        assert!(bob_owncoins[0].note.token_id == alice_token_id);
        assert!(bob_owncoins[1].note.value == BOB_INITIAL - BOB_FIRST_SEND);
        assert!(bob_owncoins[1].note.token_id == bob_token_id);

        th.assert_trees(&HOLDERS);

        // Alice and Bob decide to swap back their tokens so Alice gets back her initial
        // tokens and Bob gets his.
        info!(target: "money", "[Alice, Bob] ================");
        info!(target: "money", "[Alice, Bob] Building OtcSwap");
        info!(target: "money", "[Alice, Bob] ================");
        let alice_oc = alice_owncoins[1].clone();
        alice_owncoins.remove(1);
        assert!(alice_owncoins.len() == 1);
        let bob_oc = bob_owncoins[0].clone();
        bob_owncoins.remove(0);
        assert!(bob_owncoins.len() == 1);

        let (otc_swap_tx, otc_swap_params, fee_params) = th
            .otc_swap(&Holder::Alice, &alice_oc, &Holder::Bob, &bob_oc, current_block_height)
            .await?;

        for holder in &HOLDERS {
            info!(target: "money", "[{holder:?}] ==========================");
            info!(target: "money", "[{holder:?}] Executing AliceBob swap tx");
            info!(target: "money", "[{holder:?}] ==========================");
            th.execute_otc_swap_tx(
                holder,
                otc_swap_tx.clone(),
                &otc_swap_params,
                &fee_params,
                current_block_height,
                true,
            )
            .await?;
        }

        // Alice should now have two OwnCoins with the same token ID (ALICE)
        let mut alice_owncoins =
            th.holders.get(&Holder::Alice).unwrap().unspent_money_coins.clone();
        let mut bob_owncoins = th.holders.get(&Holder::Bob).unwrap().unspent_money_coins.clone();

        assert!(alice_owncoins.len() == 2);
        assert!(alice_owncoins[0].note.token_id == alice_token_id);
        assert!(alice_owncoins[1].note.token_id == alice_token_id);

        // Same for Bob with BOB tokens
        assert!(bob_owncoins.len() == 2);
        assert!(bob_owncoins[0].note.token_id == bob_token_id);
        assert!(bob_owncoins[1].note.token_id == bob_token_id);

        th.assert_trees(&HOLDERS);

        // Now Alice will create a new coin for herself to combine the two owncoins.
        info!(target: "money", "[Alice] ======================================================");
        info!(target: "money", "[Alice] Building Money::Transfer params for a payment to Alice");
        info!(target: "money", "[Alice] ======================================================");
        let (tx, (params, fee_params), spent_coins) = th
            .transfer(
                ALICE_INITIAL,
                &Holder::Alice,
                &Holder::Alice,
                &alice_owncoins,
                alice_token_id,
                current_block_height,
                false,
            )
            .await?;

        for coin in spent_coins {
            alice_owncoins.retain(|x| x != &coin);
        }
        assert!(alice_owncoins.is_empty());
        assert!(params.inputs.len() == 2);
        assert!(params.outputs.len() == 1);

        for holder in &HOLDERS {
            info!(target: "money", "[{holder:?}] ================================");
            info!(target: "money", "[{holder:?}] Executing Alice2Alice payment tx");
            info!(target: "money", "[{holder:?}] ================================");
            th.execute_transfer_tx(
                holder,
                tx.clone(),
                &params,
                &fee_params,
                current_block_height,
                true,
            )
            .await?;
        }

        th.assert_trees(&HOLDERS);

        // Alice should now have a single OwnCoin with her initial airdrop
        let alice_owncoins = th.holders.get(&Holder::Alice).unwrap().unspent_money_coins.clone();
        assert!(alice_owncoins.len() == 1);
        assert!(alice_owncoins[0].note.value == ALICE_INITIAL);
        assert!(alice_owncoins[0].note.token_id == alice_token_id);

        // Bob does the same
        info!(target: "money", "[Bob] ====================================================");
        info!(target: "money", "[Bob] Building Money::Transfer params for a payment to Bob");
        info!(target: "money", "[Bob] ====================================================");
        let (tx, (params, fee_params), spent_coins) = th
            .transfer(
                BOB_INITIAL,
                &Holder::Bob,
                &Holder::Bob,
                &bob_owncoins,
                bob_token_id,
                current_block_height,
                false,
            )
            .await?;

        for coin in spent_coins {
            bob_owncoins.retain(|x| x != &coin);
        }
        assert!(bob_owncoins.is_empty());
        assert!(params.inputs.len() == 2);
        assert!(params.outputs.len() == 1);

        for holder in &HOLDERS {
            info!(target: "money", "[{holder:?}] ============================");
            info!(target: "money", "[{holder:?}] Executing Bob2Bob payment tx");
            info!(target: "money", "[{holder:?}] ============================");
            th.execute_transfer_tx(
                holder,
                tx.clone(),
                &params,
                &fee_params,
                current_block_height,
                true,
            )
            .await?;
        }

        th.assert_trees(&HOLDERS);

        // Bob should now have a single OwnCoin with his initial airdrop
        let bob_owncoins = th.holders.get(&Holder::Bob).unwrap().unspent_money_coins.clone();
        assert!(bob_owncoins.len() == 1);
        assert!(bob_owncoins[0].note.value == BOB_INITIAL);
        assert!(bob_owncoins[0].note.token_id == bob_token_id);

        // Now they decide to swap all of their tokens
        info!(target: "money", "[Alice, Bob] ================");
        info!(target: "money", "[Alice, Bob] Building OtcSwap");
        info!(target: "money", "[Alice, Bob] ================");
        let mut alice_owncoins =
            th.holders.get(&Holder::Alice).unwrap().unspent_money_coins.clone();
        let mut bob_owncoins = th.holders.get(&Holder::Bob).unwrap().unspent_money_coins.clone();

        let alice_oc = alice_owncoins[0].clone();
        alice_owncoins.remove(0);
        assert!(alice_owncoins.is_empty());
        let bob_oc = bob_owncoins[0].clone();
        bob_owncoins.remove(0);
        assert!(bob_owncoins.is_empty());

        let (otc_swap_tx, otc_swap_params, fee_params) = th
            .otc_swap(&Holder::Alice, &alice_oc, &Holder::Bob, &bob_oc, current_block_height)
            .await?;

        for holder in &HOLDERS {
            info!(target: "money", "[{holder:?}] ==========================");
            info!(target: "money", "[{holder:?}] Executing AliceBob swap tx");
            info!(target: "money", "[{holder:?}] ==========================");
            th.execute_otc_swap_tx(
                holder,
                otc_swap_tx.clone(),
                &otc_swap_params,
                &fee_params,
                current_block_height,
                true,
            )
            .await?;
        }

        assert_eq!(otc_swap_params.outputs.len(), 2);

        // Alice should now have Bob's BOB tokens
        let alice_owncoins = th.holders.get(&Holder::Alice).unwrap().unspent_money_coins.clone();
        assert!(alice_owncoins.len() == 1);
        assert!(alice_owncoins[0].note.value == BOB_INITIAL);
        assert!(alice_owncoins[0].note.token_id == bob_token_id);

        // And Bob should have Alice's ALICE tokens
        let bob_owncoins = th.holders.get(&Holder::Bob).unwrap().unspent_money_coins.clone();
        assert!(bob_owncoins.len() == 1);
        assert!(bob_owncoins[0].note.value == ALICE_INITIAL);
        assert!(bob_owncoins[0].note.token_id == alice_token_id);

        th.assert_trees(&HOLDERS);

        // Thanks for reading
        Ok(())
    })
}
