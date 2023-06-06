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

//! Integration test of consensus genesis staking and unstaking for Alice.
//!
//! We first stake Alice some native tokes on genesis slot, and then she can
//! propose and unstake them a couple of times.
//!
//! With this test, we want to confirm the consensus contract state
//! transitions work for a single party and are able to be verified.

use darkfi::Result;
use log::info;

use darkfi_consensus_contract::model::REWARD;

mod harness;
use harness::{init_logger, ConsensusTestHarness, Holder};

#[async_std::test]
async fn consensus_contract_genesis_stake_unstake() -> Result<()> {
    init_logger();

    // Some numbers we want to assert
    const ALICE_INITIAL: u64 = 1000;

    // Slot to verify against
    let current_slot = 0;

    // Initialize harness
    let mut th = ConsensusTestHarness::new().await?;

    // Now Alice can craate a genesis stake transaction to mint
    // some staked coins
    info!(target: "consensus", "[Alice] =========================");
    info!(target: "consensus", "[Alice] Building genesis stake tx");
    info!(target: "consensus", "[Alice] =========================");
    let (genesis_stake_tx, genesis_stake_params) =
        th.genesis_stake_native(Holder::Alice, ALICE_INITIAL)?;

    // We are going to use alice genesis mint transaction to
    // test some malicious cases.
    info!(target: "consensus", "[Malicious] ===================================");
    info!(target: "consensus", "[Malicious] Checking duplicate genesis stake tx");
    info!(target: "consensus", "[Malicious] ===================================");
    th.execute_erroneous_genesis_stake_native_txs(
        Holder::Alice,
        vec![genesis_stake_tx.clone(), genesis_stake_tx.clone()],
        current_slot,
        1,
    )
    .await?;

    info!(target: "consensus", "[Malicious] =============================================");
    info!(target: "consensus", "[Malicious] Checking genesis stake tx not on genesis slot");
    info!(target: "consensus", "[Malicious] =============================================");
    th.execute_erroneous_genesis_stake_native_txs(
        Holder::Alice,
        vec![genesis_stake_tx.clone()],
        current_slot + 1,
        1,
    )
    .await?;
    info!(target: "consensus", "[Malicious] ===========================");
    info!(target: "consensus", "[Malicious] Malicious test cases passed");
    info!(target: "consensus", "[Malicious] ===========================");

    info!(target: "consensus", "[Faucet] ================================");
    info!(target: "consensus", "[Faucet] Executing Alice genesis stake tx");
    info!(target: "consensus", "[Faucet] ================================");
    th.execute_genesis_stake_native_tx(
        Holder::Faucet,
        genesis_stake_tx.clone(),
        &genesis_stake_params,
        current_slot,
    )
    .await?;

    info!(target: "consensus", "[Alice] ================================");
    info!(target: "consensus", "[Alice] Executing Alice genesis stake tx");
    info!(target: "consensus", "[Alice] ================================");
    th.execute_genesis_stake_native_tx(
        Holder::Alice,
        genesis_stake_tx,
        &genesis_stake_params,
        current_slot,
    )
    .await?;

    th.assert_trees();

    // Gather new staked owncoin
    let alice_staked_oc =
        th.gather_consensus_owncoin(Holder::Alice, genesis_stake_params.output, None)?;

    // Verify values match
    assert!(ALICE_INITIAL == alice_staked_oc.note.value);

    // We simulate the proposal of genesis slot
    let slot_checkpoint = th.get_slot_checkpoint_by_slot(current_slot).await?;

    // With alice's current coin value she can become the slot proposer,
    // so she creates a proposal transaction to burn her staked coin,
    // reward herself and mint the new coin.
    info!(target: "consensus", "[Alice] ====================");
    info!(target: "consensus", "[Alice] Building proposal tx");
    info!(target: "consensus", "[Alice] ====================");
    let (proposal_tx, proposal_params, proposal_secret_key) =
        th.proposal(Holder::Alice, slot_checkpoint, alice_staked_oc.clone())?;

    info!(target: "consensus", "[Faucet] ===========================");
    info!(target: "consensus", "[Faucet] Executing Alice proposal tx");
    info!(target: "consensus", "[Faucet] ===========================");
    th.execute_proposal_tx(Holder::Faucet, proposal_tx.clone(), &proposal_params, current_slot)
        .await?;

    info!(target: "consensus", "[Alice] ===========================");
    info!(target: "consensus", "[Alice] Executing Alice proposal tx");
    info!(target: "consensus", "[Alice] ===========================");
    th.execute_proposal_tx(Holder::Alice, proposal_tx, &proposal_params, current_slot).await?;

    th.assert_trees();

    // Gather new staked owncoin which includes the reward
    let alice_rewarded_staked_oc = th.gather_consensus_owncoin(
        Holder::Alice,
        proposal_params.output,
        Some(proposal_secret_key),
    )?;

    // Verify values match
    assert!((alice_staked_oc.note.value + REWARD) == alice_rewarded_staked_oc.note.value);

    // Now Alice can unstake her owncoin
    info!(target: "consensus", "[Alice] ===================");
    info!(target: "consensus", "[Alice] Building unstake tx");
    info!(target: "consensus", "[Alice] ===================");
    let (unstake_tx, unstake_params, unstake_secret_key) =
        th.unstake_native(Holder::Alice, alice_rewarded_staked_oc.clone())?;

    info!(target: "consensus", "[Faucet] ==========================");
    info!(target: "consensus", "[Faucet] Executing Alice unstake tx");
    info!(target: "consensus", "[Faucet] ==========================");
    th.execute_unstake_native_tx(Holder::Faucet, unstake_tx.clone(), &unstake_params, current_slot)
        .await?;

    info!(target: "consensus", "[Alice] ==========================");
    info!(target: "consensus", "[Alice] Executing Alice unstake tx");
    info!(target: "consensus", "[Alice] ==========================");
    th.execute_unstake_native_tx(Holder::Alice, unstake_tx, &unstake_params, current_slot).await?;

    th.assert_trees();

    // Gather new unstaked owncoin
    let alice_unstaked_oc =
        th.gather_owncoin(Holder::Alice, unstake_params.output, Some(unstake_secret_key))?;

    // Verify values match
    assert!(alice_rewarded_staked_oc.note.value == alice_unstaked_oc.note.value);

    // Statistics
    th.statistics();

    // Thanks for reading
    Ok(())
}
