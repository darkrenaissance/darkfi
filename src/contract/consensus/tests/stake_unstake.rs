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
//! The following malicious cases are also tested:
//!     1. Repeat staking coin
//!     2. Proposal before grace period
//!     3. Unstaking before grace period
//!     4. Repeat requesting unstaking coin
//!     5. Repeat unstaking coin
//!     6. Use unstaked coin in proposal
//!
//! With this test, we want to confirm the consensus contract state
//! transitions work for a single party and are able to be verified.

use darkfi::Result;
use log::info;

use darkfi_consensus_contract::model::{calculate_grace_period, EPOCH_LENGTH};
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness, TxAction};
use darkfi_sdk::pasta::pallas;

#[async_std::test]
async fn consensus_contract_stake_unstake() -> Result<()> {
    init_logger();

    // Holders this test will use
    const HOLDERS: [Holder; 2] = [Holder::Faucet, Holder::Alice];

    // Some numbers we want to assert
    const ALICE_AIRDROP: u64 = 1000;

    // Slot to verify against
    let mut current_slot = 1;

    // Initialize harness
    let mut th = TestHarness::new(&["money".to_string(), "consensus".to_string()]).await?;

    // Now Alice can airdrop some native tokens to herself
    let alice_oc =
        th.execute_airdrop(&HOLDERS, &Holder::Alice, ALICE_AIRDROP, current_slot).await?;

    // Now Alice can stake her owncoin
    let alice_staked_oc =
        th.execute_stake(&HOLDERS, &Holder::Alice, current_slot, &alice_oc, 86).await?;

    // We progress after grace period
    current_slot += (calculate_grace_period() * EPOCH_LENGTH) + EPOCH_LENGTH;
    let slot = th.generate_slot(current_slot).await?;

    // With alice's current coin value she can become the slot proposer,
    // so she creates a proposal transaction to burn her staked coin,
    // reward herself and mint the new coin.
    let alice_rewarded_staked_oc =
        th.execute_proposal(&HOLDERS, &Holder::Alice, current_slot, slot, &alice_staked_oc).await?;

    // We progress one slot
    current_slot += 1;
    th.generate_slot(current_slot).await?;

    // Alice can request for her owncoin to get unstaked
    let alice_unstake_request_oc = th
        .execute_unstake_request(&HOLDERS, &Holder::Alice, current_slot, &alice_rewarded_staked_oc)
        .await?;

    // We progress after grace period
    current_slot += (calculate_grace_period() * EPOCH_LENGTH) + EPOCH_LENGTH;

    // Now Alice can unstake her owncoin
    let alice_unstaked_oc = th
        .execute_unstake(&HOLDERS, &Holder::Alice, current_slot, &alice_unstake_request_oc)
        .await?;

    // Now Alice can stake her unstaked owncoin again to try some mallicious cases
    let alice_staked_oc =
        th.execute_stake(&HOLDERS, &Holder::Alice, current_slot, &alice_unstaked_oc, 262).await?;

    // Alice tries to stake her coin again
    info!(target: "consensus", "[Malicious] ===========================");
    info!(target: "consensus", "[Malicious] Checking staking coin again");
    info!(target: "consensus", "[Malicious] ===========================");
    let (stake_tx, _, _) =
        th.stake(&Holder::Alice, current_slot, &alice_unstaked_oc, pallas::Base::from(262)).await?;
    th.execute_erroneous_txs(
        TxAction::ConsensusStake,
        &Holder::Alice,
        &vec![stake_tx],
        current_slot,
        1,
    )
    .await?;

    // We progress one slot
    current_slot += 1;
    let slot = th.generate_slot(current_slot).await?;

    // Since alice didn't wait for the grace period to pass, her proposal should fail
    info!(target: "consensus", "[Malicious] =====================================");
    info!(target: "consensus", "[Malicious] Checking proposal before grace period");
    info!(target: "consensus", "[Malicious] =====================================");
    let (proposal_tx, _, _, _) = th.proposal(&Holder::Alice, slot, &alice_staked_oc).await?;
    th.execute_erroneous_txs(
        TxAction::ConsensusProposal,
        &Holder::Alice,
        &vec![proposal_tx],
        current_slot,
        1,
    )
    .await?;

    // or be able to unstake the coin
    info!(target: "consensus", "[Malicious] ======================================");
    info!(target: "consensus", "[Malicious] Checking unstaking before grace period");
    info!(target: "consensus", "[Malicious] ======================================");
    let (unstake_request_tx, _, _, _) =
        th.unstake_request(&Holder::Alice, current_slot, &alice_staked_oc).await?;
    th.execute_erroneous_txs(
        TxAction::ConsensusUnstakeRequest,
        &Holder::Alice,
        &vec![unstake_request_tx],
        current_slot,
        1,
    )
    .await?;

    // We progress after grace period
    current_slot += (calculate_grace_period() * EPOCH_LENGTH) + EPOCH_LENGTH;

    // Alice can request for her owncoin to get unstaked
    let alice_unstake_request_oc = th
        .execute_unstake_request(&HOLDERS, &Holder::Alice, current_slot, &alice_staked_oc)
        .await?;

    info!(target: "consensus", "[Malicious] =====================================");
    info!(target: "consensus", "[Malicious] Checking request unstaking coin again");
    info!(target: "consensus", "[Malicious] =====================================");
    let (unstake_request_tx, _, _, _) =
        th.unstake_request(&Holder::Alice, current_slot, &alice_staked_oc).await?;
    th.execute_erroneous_txs(
        TxAction::ConsensusUnstakeRequest,
        &Holder::Alice,
        &vec![unstake_request_tx],
        current_slot,
        1,
    )
    .await?;

    // We progress after grace period
    current_slot += (calculate_grace_period() * EPOCH_LENGTH) + EPOCH_LENGTH;

    // Now Alice can unstake her owncoin
    let alice_unstaked_oc = th
        .execute_unstake(&HOLDERS, &Holder::Alice, current_slot, &alice_unstake_request_oc)
        .await?;

    info!(target: "consensus", "[Malicious] =============================");
    info!(target: "consensus", "[Malicious] Checking unstaking coin again");
    info!(target: "consensus", "[Malicious] =============================");
    let (unstake_tx, _, _) = th.unstake(&Holder::Alice, &alice_unstake_request_oc)?;
    th.execute_erroneous_txs(
        TxAction::ConsensusUnstake,
        &Holder::Alice,
        &vec![unstake_tx],
        current_slot,
        1,
    )
    .await?;

    // Now Alice can stake her unstaked owncoin again
    let alice_staked_oc =
        th.execute_stake(&HOLDERS, &Holder::Alice, current_slot, &alice_unstaked_oc, 70).await?;

    // We progress after grace period
    current_slot += (calculate_grace_period() * EPOCH_LENGTH) + EPOCH_LENGTH;

    // Alice can request for her owncoin to get unstaked
    let alice_unstake_request_oc = th
        .execute_unstake_request(&HOLDERS, &Holder::Alice, current_slot, &alice_staked_oc)
        .await?;

    // Now we will test if we can reuse token in proposal
    current_slot += 1;
    let slot = th.generate_slot(current_slot).await?;

    info!(target: "consensus", "[Malicious] ========================================");
    info!(target: "consensus", "[Malicious] Checking using unstaked coin in proposal");
    info!(target: "consensus", "[Malicious] ========================================");
    let (proposal_tx, _, _, _) = th.proposal(&Holder::Alice, slot, &alice_staked_oc).await?;
    th.execute_erroneous_txs(
        TxAction::ConsensusProposal,
        &Holder::Alice,
        &vec![proposal_tx],
        current_slot,
        1,
    )
    .await?;

    // We progress after grace period
    current_slot += (calculate_grace_period() * EPOCH_LENGTH) + EPOCH_LENGTH;

    // Now Alice can unstake her owncoin
    th.execute_unstake(&HOLDERS, &Holder::Alice, current_slot, &alice_unstake_request_oc).await?;

    // Statistics
    th.statistics();

    // Thanks for reading
    Ok(())
}
