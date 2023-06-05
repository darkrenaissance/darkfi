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

//! Integration test of consensus staking and unstaking for Alice.
//!
//! We first airdrop Alice native tokes, and then she can stake,
//! propose and unstake them a couple of times.
//!
//! With this test, we want to confirm the consensus contract state
//! transitions work for a single party and are able to be verified.
//!
//! TODO: Malicious cases

use darkfi::Result;
use log::info;

use darkfi_consensus_contract::model::REWARD;

mod harness;
use harness::{init_logger, ConsensusTestHarness, Holder};

#[async_std::test]
async fn consensus_contract_stake_unstake() -> Result<()> {
    init_logger();

    // Some numbers we want to assert
    const ALICE_AIRDROP: u64 = 1000;

    // Slot to verify against
    let current_slot = 0;
    let current_epoch = 0;

    // Initialize harness
    let mut th = ConsensusTestHarness::new().await?;
    info!(target: "consensus", "[Faucet] =========================");
    info!(target: "consensus", "[Faucet] Building Alice airdrop tx");
    info!(target: "consensus", "[Faucet] =========================");
    let (airdrop_tx, airdrop_params) = th.airdrop_native(ALICE_AIRDROP, Holder::Alice)?;

    info!(target: "consensus", "[Faucet] ==========================");
    info!(target: "consensus", "[Faucet] Executing Alice airdrop tx");
    info!(target: "consensus", "[Faucet] ==========================");
    th.execute_airdrop_native_tx(Holder::Faucet, airdrop_tx.clone(), &airdrop_params, current_slot)
        .await?;

    info!(target: "consensus", "[Alice] ==========================");
    info!(target: "consensus", "[Alice] Executing Alice airdrop tx");
    info!(target: "consensus", "[Alice] ==========================");
    th.execute_airdrop_native_tx(Holder::Alice, airdrop_tx, &airdrop_params, current_slot).await?;

    th.assert_trees();

    // Gather new owncoin
    let alice_oc = th.gather_owncoin(Holder::Alice, airdrop_params.outputs[0].clone(), None)?;

    // Now Alice can stake her owncoin
    info!(target: "consensus", "[Alice] =================");
    info!(target: "consensus", "[Alice] Building stake tx");
    info!(target: "consensus", "[Alice] =================");
    let (stake_tx, stake_params, stake_secret_key) =
        th.stake_native(Holder::Alice, current_epoch, alice_oc.clone())?;

    info!(target: "consensus", "[Faucet] ========================");
    info!(target: "consensus", "[Faucet] Executing Alice stake tx");
    info!(target: "consensus", "[Faucet] ========================");
    th.execute_stake_native_tx(Holder::Faucet, stake_tx.clone(), &stake_params, current_slot)
        .await?;

    info!(target: "consensus", "[Alice] ========================");
    info!(target: "consensus", "[Alice] Executing Alice stake tx");
    info!(target: "consensus", "[Alice] ========================");
    th.execute_stake_native_tx(Holder::Alice, stake_tx, &stake_params, current_slot).await?;

    th.assert_trees();

    // Gather new staked owncoin
    let alice_staked_oc =
        th.gather_consensus_owncoin(Holder::Alice, stake_params.output, Some(stake_secret_key))?;

    // Verify values match
    assert!(alice_oc.note.value == alice_staked_oc.note.value);

    // We simulate the proposal of genesis slot
    let slot_checkpoint = th.get_slot_checkpoints_by_slot(current_slot).await?;

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
