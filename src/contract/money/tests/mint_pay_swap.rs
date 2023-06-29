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
use log::info;

#[async_std::test]
async fn mint_pay_swap() -> Result<()> {
    init_logger();

    // Holders this test will use
    const HOLDERS: [Holder; 3] = [Holder::Faucet, Holder::Alice, Holder::Bob];

    // Some numbers we want to assert
    const ALICE_INITIAL: u64 = 100;
    const BOB_INITIAL: u64 = 200;

    // Alice = 50 ALICE
    // Bob = 200 BOB + 50 ALICE
    const ALICE_FIRST_SEND: u64 = ALICE_INITIAL - 50;
    // Alice = 50 ALICE + 180 BOB
    // Bob = 20 BOB + 50 ALICE
    const BOB_FIRST_SEND: u64 = BOB_INITIAL - 20;

    // Slot to verify against
    let current_slot = 0;

    // Initialize harness
    let mut th = TestHarness::new(&["money".to_string()]).await?;

    let mut alice_owncoins = vec![];
    let mut bob_owncoins = vec![];

    info!(target: "money", "[Alice] ================================");
    info!(target: "money", "[Alice] Building token mint tx for Alice");
    info!(target: "money", "[Alice] ================================");
    let (mint_tx, params) = th.token_mint(ALICE_INITIAL, Holder::Alice, Holder::Alice)?;

    info!(target: "money", "[Faucet] =============================");
    info!(target: "money", "[Faucet] Executing Alice token mint tx");
    info!(target: "money", "[Faucet] =============================");
    th.execute_token_mint_tx(Holder::Faucet, &mint_tx, &params, current_slot).await?;

    info!(target: "money", "[Alice] ===========================");
    info!(target: "money", "[Alice] Executing Bob token mint tx");
    info!(target: "money", "[Alice] ===========================");
    th.execute_token_mint_tx(Holder::Alice, &mint_tx, &params, current_slot).await?;

    info!(target: "money", "[Bob] =============================");
    info!(target: "money", "[Bob] Executing Alice token mint tx");
    info!(target: "money", "[Bob] =============================");
    th.execute_token_mint_tx(Holder::Bob, &mint_tx, &params, current_slot).await?;

    th.assert_trees(&HOLDERS);

    // Alice gathers her new owncoin
    let alice_oc = th.gather_owncoin(Holder::Alice, params.output, None)?;
    let alice_token_id = alice_oc.note.token_id;
    alice_owncoins.push(alice_oc);

    info!(target: "money", "[Bob] ==============================");
    info!(target: "money", "[Bob] Building token mint tx for Bob");
    info!(target: "money", "[Bob] ==============================");
    let (mint_tx, params) = th.token_mint(BOB_INITIAL, Holder::Bob, Holder::Bob)?;

    info!(target: "money", "[Faucet] ===========================");
    info!(target: "money", "[Faucet] Executing Bob token mint tx");
    info!(target: "money", "[Faucet] ===========================");
    th.execute_token_mint_tx(Holder::Faucet, &mint_tx, &params, current_slot).await?;

    info!(target: "money", "[Alice] =============================");
    info!(target: "money", "[Alice] Executing Alice token mint tx");
    info!(target: "money", "[Alice] =============================");
    th.execute_token_mint_tx(Holder::Alice, &mint_tx, &params, current_slot).await?;

    info!(target: "money", "[Bob] ===========================");
    info!(target: "money", "[Bob] Executing Bob token mint tx");
    info!(target: "money", "[Bob] ===========================");
    th.execute_token_mint_tx(Holder::Bob, &mint_tx, &params, current_slot).await?;

    th.assert_trees(&HOLDERS);

    // Bob  gathers hist new owncoin
    let bob_oc = th.gather_owncoin(Holder::Bob, params.output, None)?;
    let bob_token_id = bob_oc.note.token_id;
    bob_owncoins.push(bob_oc);

    // Now Alice can send a little bit of funds to Bob
    info!(target: "money", "[Alice] ====================================================");
    info!(target: "money", "[Alice] Building Money::Transfer params for a payment to Bob");
    info!(target: "money", "[Alice] ====================================================");
    let (transfer_tx, transfer_params, spent_coins) =
        th.transfer(ALICE_FIRST_SEND, Holder::Alice, Holder::Bob, &alice_owncoins, alice_token_id)?;

    // Validating transfer params
    assert!(transfer_params.inputs.len() == 1);
    assert!(transfer_params.outputs.len() == 2);
    assert!(spent_coins.len() == 1);
    alice_owncoins.retain(|x| x != &spent_coins[0]);
    assert!(alice_owncoins.is_empty());

    info!(target: "money", "[Faucet] ==============================");
    info!(target: "money", "[Faucet] Executing Alice2Bob payment tx");
    info!(target: "money", "[Faucet] ==============================");
    th.execute_transfer_tx(Holder::Faucet, &transfer_tx, &transfer_params, current_slot, true)
        .await?;

    info!(target: "money", "[Alice] ==============================");
    info!(target: "money", "[Alice] Executing Alice2Bob payment tx");
    info!(target: "money", "[Alice] ==============================");
    th.execute_transfer_tx(Holder::Alice, &transfer_tx, &transfer_params, current_slot, false)
        .await?;

    info!(target: "money", "[Bob] ==============================");
    info!(target: "money", "[Bob] Executing Alice2Bob payment tx");
    info!(target: "money", "[Bob] ==============================");
    th.execute_transfer_tx(Holder::Bob, &transfer_tx, &transfer_params, current_slot, false)
        .await?;

    // Alice should now have one OwnCoin with the change from the above transaction.
    let alice_oc = th.gather_owncoin_at_index(Holder::Alice, &transfer_params.outputs, 0)?;
    alice_owncoins.push(alice_oc);

    // Bob should now have this new one.
    let bob_oc = th.gather_owncoin_at_index(Holder::Bob, &transfer_params.outputs, 1)?;
    bob_owncoins.push(bob_oc);

    assert!(alice_owncoins.len() == 1);
    assert!(bob_owncoins.len() == 2);

    th.assert_trees(&HOLDERS);

    // Bob can send a little bit to Alice as well
    info!(target: "money", "[Bob] ======================================================");
    info!(target: "money", "[Bob] Building Money::Transfer params for a payment to Alice");
    info!(target: "money", "[Bob] ======================================================");
    let mut bob_owncoins_tmp = bob_owncoins.clone();
    bob_owncoins_tmp.retain(|x| x.note.token_id == bob_token_id);
    let (transfer_tx, transfer_params, spent_coins) =
        th.transfer(BOB_FIRST_SEND, Holder::Bob, Holder::Alice, &bob_owncoins_tmp, bob_token_id)?;

    // Validating transfer params
    assert!(transfer_params.inputs.len() == 1);
    assert!(transfer_params.outputs.len() == 2);
    assert!(spent_coins.len() == 1);
    bob_owncoins.retain(|x| x != &spent_coins[0]);
    assert!(bob_owncoins.len() == 1);

    info!(target: "money", "[Faucet] ==============================");
    info!(target: "money", "[Faucet] Executing Bob2Alice payment tx");
    info!(target: "money", "[Faucet] ==============================");
    th.execute_transfer_tx(Holder::Faucet, &transfer_tx, &transfer_params, current_slot, true)
        .await?;

    info!(target: "money", "[Alice] ==============================");
    info!(target: "money", "[Alice] Executing Bob2Alice payment tx");
    info!(target: "money", "[Alice] ==============================");
    th.execute_transfer_tx(Holder::Alice, &transfer_tx, &transfer_params, current_slot, false)
        .await?;

    info!(target: "money", "[Bob] ==============================");
    info!(target: "money", "[Bob] Executing Bob2Alice payment tx");
    info!(target: "money", "[Bob] ==============================");
    th.execute_transfer_tx(Holder::Bob, &transfer_tx, &transfer_params, current_slot, false)
        .await?;

    // Alice should now have two OwnCoins
    let alice_oc = th.gather_owncoin_at_index(Holder::Alice, &transfer_params.outputs, 1)?;
    alice_owncoins.push(alice_oc);

    // Bob should have two with the change from the above tx
    let bob_oc = th.gather_owncoin_at_index(Holder::Bob, &transfer_params.outputs, 0)?;
    bob_owncoins.push(bob_oc);

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

    let (otc_swap_tx, otc_swap_params) =
        th.otc_swap(Holder::Alice, alice_oc, Holder::Bob, bob_oc)?;

    info!(target: "money", "[Faucet] ==========================");
    info!(target: "money", "[Faucet] Executing AliceBob swap tx");
    info!(target: "money", "[Faucet] ==========================");
    th.execute_otc_swap_tx(Holder::Faucet, &otc_swap_tx, &otc_swap_params, current_slot, true)
        .await?;

    info!(target: "money", "[Alice] ==========================");
    info!(target: "money", "[Alice] Executing AliceBob swap tx");
    info!(target: "money", "[Alice] ==========================");
    th.execute_otc_swap_tx(Holder::Alice, &otc_swap_tx, &otc_swap_params, current_slot, false)
        .await?;

    info!(target: "money", "[Bob] ==========================");
    info!(target: "money", "[Bob] Executing AliceBob swap tx");
    info!(target: "money", "[Bob] ==========================");
    th.execute_otc_swap_tx(Holder::Bob, &otc_swap_tx, &otc_swap_params, current_slot, false)
        .await?;

    // Alice should now have two OwnCoins with the same token ID (ALICE)
    let alice_oc = th.gather_owncoin_at_index(Holder::Alice, &otc_swap_params.outputs, 0)?;
    alice_owncoins.push(alice_oc);

    assert!(alice_owncoins.len() == 2);
    assert!(alice_owncoins[0].note.token_id == alice_token_id);
    assert!(alice_owncoins[1].note.token_id == alice_token_id);

    // Same for Bob with BOB tokens
    let bob_oc = th.gather_owncoin_at_index(Holder::Bob, &otc_swap_params.outputs, 1)?;
    bob_owncoins.push(bob_oc);

    assert!(bob_owncoins.len() == 2);
    assert!(bob_owncoins[0].note.token_id == bob_token_id);
    assert!(bob_owncoins[1].note.token_id == bob_token_id);

    th.assert_trees(&HOLDERS);

    // Now Alice will create a new coin for herself to combine the two owncoins.
    info!(target: "money", "[Alice] ======================================================");
    info!(target: "money", "[Alice] Building Money::Transfer params for a payment to Alice");
    info!(target: "money", "[Alice] ======================================================");
    let (tx, params, spent_coins) =
        th.transfer(ALICE_INITIAL, Holder::Alice, Holder::Alice, &alice_owncoins, alice_token_id)?;

    for coin in spent_coins {
        alice_owncoins.retain(|x| x != &coin);
    }
    assert!(alice_owncoins.is_empty());
    assert!(params.inputs.len() == 2);
    assert!(params.outputs.len() == 1);

    info!(target: "money", "[Faucet] ================================");
    info!(target: "money", "[Faucet] Executing Alice2Alice payment tx");
    info!(target: "money", "[Faucet] ================================");
    th.execute_transfer_tx(Holder::Faucet, &tx, &params, current_slot, true).await?;

    info!(target: "money", "[Alice] ================================");
    info!(target: "money", "[Alice] Executing Alice2Alice payment tx");
    info!(target: "money", "[Alice] ================================");
    th.execute_transfer_tx(Holder::Alice, &tx, &params, current_slot, true).await?;

    info!(target: "money", "[Bob] ================================");
    info!(target: "money", "[Bob] Executing Alice2Alice payment tx");
    info!(target: "money", "[Bob] ================================");
    th.execute_transfer_tx(Holder::Bob, &tx, &params, current_slot, true).await?;

    th.assert_trees(&HOLDERS);

    // Alice should now have a single OwnCoin with her initial airdrop
    let alice_oc = th.gather_owncoin(Holder::Alice, params.outputs[0].clone(), None)?;
    alice_owncoins.push(alice_oc);

    assert!(alice_owncoins.len() == 1);
    assert!(alice_owncoins[0].note.value == ALICE_INITIAL);
    assert!(alice_owncoins[0].note.token_id == alice_token_id);

    // Bob does the same
    info!(target: "money", "[Bob] ====================================================");
    info!(target: "money", "[Bob] Building Money::Transfer params for a payment to Bob");
    info!(target: "money", "[Bob] ====================================================");
    let (tx, params, spent_coins) =
        th.transfer(BOB_INITIAL, Holder::Bob, Holder::Bob, &bob_owncoins, bob_token_id)?;

    for coin in spent_coins {
        bob_owncoins.retain(|x| x != &coin);
    }
    assert!(bob_owncoins.is_empty());
    assert!(params.inputs.len() == 2);
    assert!(params.outputs.len() == 1);

    info!(target: "money", "[Faucet] ============================");
    info!(target: "money", "[Faucet] Executing Bob2Bob payment tx");
    info!(target: "money", "[Faucet] ============================");
    th.execute_transfer_tx(Holder::Faucet, &tx, &params, current_slot, true).await?;

    info!(target: "money", "[Alice] ============================");
    info!(target: "money", "[Alice] Executing Bob2Bob payment tx");
    info!(target: "money", "[Alice] ============================");
    th.execute_transfer_tx(Holder::Alice, &tx, &params, current_slot, true).await?;

    info!(target: "money", "[Bob] ============================");
    info!(target: "money", "[Bob] Executing Bob2Bob payment tx");
    info!(target: "money", "[Bob] ============================");
    th.execute_transfer_tx(Holder::Bob, &tx, &params, current_slot, true).await?;

    th.assert_trees(&HOLDERS);

    // Bob should now have a single OwnCoin with his initial airdrop
    let bob_oc = th.gather_owncoin(Holder::Bob, params.outputs[0].clone(), None)?;
    bob_owncoins.push(bob_oc);

    assert!(bob_owncoins.len() == 1);
    assert!(bob_owncoins[0].note.value == BOB_INITIAL);
    assert!(bob_owncoins[0].note.token_id == bob_token_id);

    // Now they decide to swap all of their tokens
    info!(target: "money", "[Alice, Bob] ================");
    info!(target: "money", "[Alice, Bob] Building OtcSwap");
    info!(target: "money", "[Alice, Bob] ================");
    let alice_oc = alice_owncoins[0].clone();
    alice_owncoins.remove(0);
    assert!(alice_owncoins.is_empty());
    let bob_oc = bob_owncoins[0].clone();
    bob_owncoins.remove(0);
    assert!(bob_owncoins.is_empty());

    let (otc_swap_tx, otc_swap_params) =
        th.otc_swap(Holder::Alice, alice_oc, Holder::Bob, bob_oc)?;

    info!(target: "money", "[Faucet] ==========================");
    info!(target: "money", "[Faucet] Executing AliceBob swap tx");
    info!(target: "money", "[Faucet] ==========================");
    th.execute_otc_swap_tx(Holder::Faucet, &otc_swap_tx, &otc_swap_params, current_slot, true)
        .await?;

    info!(target: "money", "[Alice] ==========================");
    info!(target: "money", "[Alice] Executing AliceBob swap tx");
    info!(target: "money", "[Alice] ==========================");
    th.execute_otc_swap_tx(Holder::Alice, &otc_swap_tx, &otc_swap_params, current_slot, false)
        .await?;

    info!(target: "money", "[Bob] ==========================");
    info!(target: "money", "[Bob] Executing AliceBob swap tx");
    info!(target: "money", "[Bob] ==========================");
    th.execute_otc_swap_tx(Holder::Bob, &otc_swap_tx, &otc_swap_params, current_slot, false)
        .await?;

    // Alice should now have Bob's BOB tokens
    let alice_oc = th.gather_owncoin_at_index(Holder::Alice, &otc_swap_params.outputs, 0)?;
    alice_owncoins.push(alice_oc);

    assert!(alice_owncoins.len() == 1);
    assert!(alice_owncoins[0].note.value == BOB_INITIAL);
    assert!(alice_owncoins[0].note.token_id == bob_token_id);

    // And Bob should have Alice's ALICE tokens
    let bob_oc = th.gather_owncoin_at_index(Holder::Bob, &otc_swap_params.outputs, 1)?;
    bob_owncoins.push(bob_oc);

    assert!(bob_owncoins.len() == 1);
    assert!(bob_owncoins[0].note.value == ALICE_INITIAL);
    assert!(bob_owncoins[0].note.token_id == alice_token_id);

    th.assert_trees(&HOLDERS);

    // Statistics
    th.statistics();

    // Thanks for reading
    Ok(())
}
