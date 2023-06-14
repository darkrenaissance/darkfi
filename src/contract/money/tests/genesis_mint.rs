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
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness};
use log::info;

#[async_std::test]
async fn genesis_mint() -> Result<()> {
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
    let (genesis_mint_tx, genesis_mint_params) = th.genesis_mint(Holder::Alice, ALICE_INITIAL)?;

    // We are going to use alice genesis mint transaction to
    // test some malicious cases.
    info!(target: "money", "[Malicious] ==================================");
    info!(target: "money", "[Malicious] Checking duplicate genesis mint tx");
    info!(target: "money", "[Malicious] ==================================");
    th.execute_erroneous_genesis_mint_tx(
        Holder::Alice,
        vec![genesis_mint_tx.clone(), genesis_mint_tx.clone()],
        current_slot,
        1,
    )
    .await?;

    info!(target: "money", "[Malicious] ============================================");
    info!(target: "money", "[Malicious] Checking genesis mint tx not on genesis slot");
    info!(target: "money", "[Malicious] ============================================");
    th.execute_erroneous_genesis_mint_tx(
        Holder::Alice,
        vec![genesis_mint_tx.clone()],
        current_slot + 1,
        1,
    )
    .await?;
    info!(target: "money", "[Malicious] ===========================");
    info!(target: "money", "[Malicious] Malicious test cases passed");
    info!(target: "money", "[Malicious] ===========================");

    info!(target: "money", "[Faucet] ===============================");
    info!(target: "money", "[Faucet] Executing Alice genesis mint tx");
    info!(target: "money", "[Faucet] ===============================");
    th.execute_genesis_mint_tx(
        Holder::Faucet,
        &genesis_mint_tx,
        &genesis_mint_params,
        current_slot,
    )
    .await?;

    info!(target: "money", "[Alice] ===============================");
    info!(target: "money", "[Alice] Executing Alice genesis mint tx");
    info!(target: "money", "[Alice] ===============================");
    th.execute_genesis_mint_tx(Holder::Alice, &genesis_mint_tx, &genesis_mint_params, current_slot)
        .await?;

    info!(target: "money", "[Bob] ===============================");
    info!(target: "money", "[Bob] Executing Alice genesis mint tx");
    info!(target: "money", "[Bob] ===============================");
    th.execute_genesis_mint_tx(Holder::Bob, &genesis_mint_tx, &genesis_mint_params, current_slot)
        .await?;

    th.assert_trees(&HOLDERS);

    // Alice gathers her new owncoin
    let alice_oc = th.gather_owncoin(Holder::Alice, genesis_mint_params.output, None)?;
    alice_owncoins.push(alice_oc.clone());

    info!(target: "money", "[Bob] ========================");
    info!(target: "money", "[Bob] Building genesis mint tx");
    info!(target: "money", "[Bob] ========================");
    let (genesis_mint_tx, genesis_mint_params) = th.genesis_mint(Holder::Bob, BOB_INITIAL)?;

    info!(target: "money", "[Faucet] ===============================");
    info!(target: "money", "[Faucet] Executing Bob genesis mint tx");
    info!(target: "money", "[Faucet] ===============================");
    th.execute_genesis_mint_tx(
        Holder::Faucet,
        &genesis_mint_tx,
        &genesis_mint_params,
        current_slot,
    )
    .await?;

    info!(target: "money", "[Alice] ===============================");
    info!(target: "money", "[Alice] Executing Bob genesis mint tx");
    info!(target: "money", "[Alice] ===============================");
    th.execute_genesis_mint_tx(Holder::Alice, &genesis_mint_tx, &genesis_mint_params, current_slot)
        .await?;

    info!(target: "money", "[Bob] ===============================");
    info!(target: "money", "[Bob] Executing Bob genesis mint tx");
    info!(target: "money", "[Bob] ===============================");
    th.execute_genesis_mint_tx(Holder::Bob, &genesis_mint_tx, &genesis_mint_params, current_slot)
        .await?;

    th.assert_trees(&HOLDERS);

    // Bob gathers his new owncoin
    let bob_oc = th.gather_owncoin(Holder::Bob, genesis_mint_params.output, None)?;
    bob_owncoins.push(bob_oc);

    // Now Alice can send a little bit of funds to Bob
    info!(target: "money", "[Alice] ====================================================");
    info!(target: "money", "[Alice] Building Money::Transfer params for a payment to Bob");
    info!(target: "money", "[Alice] ====================================================");
    let (transfer_tx, transfer_params) =
        th.transfer(ALICE_SEND, Holder::Alice, Holder::Bob, &alice_oc)?;

    // Validating transfer params
    assert!(transfer_params.inputs.len() == 1);
    assert!(transfer_params.outputs.len() == 2);
    alice_owncoins.retain(|x| x != &alice_oc);
    assert!(alice_owncoins.is_empty());

    info!(target: "money", "[Faucet] ==============================");
    info!(target: "money", "[Faucet] Executing Alice2Bob payment tx");
    info!(target: "money", "[Faucet] ==============================");
    th.execute_transfer_tx(Holder::Faucet, &transfer_tx, &transfer_params, current_slot).await?;

    info!(target: "money", "[Alice] ==============================");
    info!(target: "money", "[Alice] Executing Alice2Bob payment tx");
    info!(target: "money", "[Alice] ==============================");
    th.execute_transfer_tx(Holder::Alice, &transfer_tx, &transfer_params, current_slot).await?;

    info!(target: "money", "[Bob] ==============================");
    info!(target: "money", "[Bob] Executing Alice2Bob payment tx");
    info!(target: "money", "[Bob] ==============================");
    th.execute_transfer_tx(Holder::Bob, &transfer_tx, &transfer_params, current_slot).await?;

    th.assert_trees(&HOLDERS);

    // Alice should now have one OwnCoin with the change from the above transaction.
    let alice_oc = th.gather_owncoin(Holder::Alice, transfer_params.outputs[0].clone(), None)?;
    alice_owncoins.push(alice_oc);

    // Bob should have his old one, and this new one.
    let bob_oc = th.gather_owncoin(Holder::Bob, transfer_params.outputs[1].clone(), None)?;
    bob_owncoins.push(bob_oc);

    assert!(alice_owncoins.len() == 1);
    assert!(bob_owncoins.len() == 2);

    // Bob can send a little bit to Alice as well
    info!(target: "money", "[Bob] ======================================================");
    info!(target: "money", "[Bob] Building Money::Transfer params for a payment to Alice");
    info!(target: "money", "[Bob] ======================================================");
    let bob_oc = bob_owncoins[0].clone();
    let (transfer_tx, transfer_params) =
        th.transfer(BOB_SEND, Holder::Bob, Holder::Alice, &bob_oc)?;

    // Validating transfer params
    assert!(transfer_params.inputs.len() == 1);
    assert!(transfer_params.outputs.len() == 2);
    bob_owncoins.retain(|x| x != &bob_oc);
    assert!(bob_owncoins.len() == 1);

    info!(target: "money", "[Faucet] ==============================");
    info!(target: "money", "[Faucet] Executing Bob2Alice payment tx");
    info!(target: "money", "[Faucet] ==============================");
    th.execute_transfer_tx(Holder::Faucet, &transfer_tx, &transfer_params, current_slot).await?;

    info!(target: "money", "[Alice] ==============================");
    info!(target: "money", "[Alice] Executing Bob2Alice payment tx");
    info!(target: "money", "[Alice] ==============================");
    th.execute_transfer_tx(Holder::Alice, &transfer_tx, &transfer_params, current_slot).await?;

    info!(target: "money", "[Bob] ==================+===========");
    info!(target: "money", "[Bob] Executing Bob2Alice payment tx");
    info!(target: "money", "[Bob] ==================+===========");
    th.execute_transfer_tx(Holder::Bob, &transfer_tx, &transfer_params, current_slot).await?;

    th.assert_trees(&HOLDERS);

    // Alice should now have two OwnCoins
    let alice_oc = th.gather_owncoin(Holder::Alice, transfer_params.outputs[1].clone(), None)?;
    alice_owncoins.push(alice_oc);

    // Bob should have two with the change from the above tx
    let bob_oc = th.gather_owncoin(Holder::Bob, transfer_params.outputs[0].clone(), None)?;
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
}
