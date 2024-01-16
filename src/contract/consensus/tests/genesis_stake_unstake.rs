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

//! Integration test of consensus genesis staking and unstaking for Alice.
//!
//! We first stake Alice some native tokes on genesis slot, and then she can
//! propose and unstake them a couple of times.
//!
//! With this test, we want to confirm the consensus contract state
//! transitions work for a single party and are able to be verified.

use darkfi::Result;
use log::info;

use darkfi_consensus_contract::model::{calculate_grace_period, EPOCH_LENGTH};
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness, TxAction};

#[test]
fn consensus_contract_genesis_stake_unstake() -> Result<()> {
    smol::block_on(async {
        init_logger();

        // Holders this test will use
        const HOLDERS: [Holder; 2] = [Holder::Faucet, Holder::Alice];

        // Some numbers we want to assert
        const ALICE_INITIAL: u64 = 1000;

        // Slot to verify against
        let mut current_slot = 0;

        // Initialize harness
        let mut th =
            TestHarness::new(&["money".to_string(), "consensus".to_string()], false).await?;

        // Now Alice can create a genesis stake transaction to mint
        // some staked coins
        info!(target: "consensus", "[Alice] =========================");
        info!(target: "consensus", "[Alice] Building genesis stake tx");
        info!(target: "consensus", "[Alice] =========================");
        let (genesis_stake_tx, genesis_stake_params) =
            th.genesis_stake(&Holder::Alice, ALICE_INITIAL)?;

        // We are going to use alice genesis mint transaction to
        // test some malicious cases.
        info!(target: "consensus", "[Malicious] ===================================");
        info!(target: "consensus", "[Malicious] Checking duplicate genesis stake tx");
        info!(target: "consensus", "[Malicious] ===================================");
        th.execute_erroneous_txs(
            TxAction::ConsensusGenesisStake,
            &Holder::Alice,
            &[genesis_stake_tx.clone(), genesis_stake_tx.clone()],
            current_slot,
            1,
        )
        .await?;

        info!(target: "consensus", "[Malicious] =============================================");
        info!(target: "consensus", "[Malicious] Checking genesis stake tx not on genesis slot");
        info!(target: "consensus", "[Malicious] =============================================");
        th.execute_erroneous_txs(
            TxAction::ConsensusGenesisStake,
            &Holder::Alice,
            &[genesis_stake_tx.clone()],
            current_slot + 1,
            1,
        )
        .await?;

        for holder in &HOLDERS {
            info!(target: "consensus", "[{holder:?}] ================================");
            info!(target: "consensus", "[{holder:?}] Executing Alice genesis stake tx");
            info!(target: "consensus", "[{holder:?}] ================================");
            th.execute_genesis_stake_tx(
                holder,
                &genesis_stake_tx,
                &genesis_stake_params,
                current_slot,
            )
            .await?;
        }

        th.assert_trees(&HOLDERS);

        // Gather new staked owncoin
        let alice_staked_oc =
            th.gather_consensus_staked_owncoin(&Holder::Alice, &genesis_stake_params.output, None)?;

        // Verify values match
        assert!(ALICE_INITIAL == alice_staked_oc.note.value);

        // We simulate the proposal of genesis slot
        // We progress 1 slot and simulate its proposal
        current_slot += 1;
        let slot = th.generate_slot(current_slot).await?;

        // With alice's current coin value she can become the slot proposer,
        // so she creates a proposal transaction to burn her staked coin,
        // reward herself and mint the new coin.
        let alice_rewarded_staked_oc = th
            .execute_proposal(&HOLDERS, &Holder::Alice, current_slot, slot, &alice_staked_oc)
            .await?;

        // We progress after grace period
        current_slot += calculate_grace_period() * EPOCH_LENGTH;
        th.generate_slot(current_slot).await?;

        // Alice can request for her owncoin to get unstaked
        let alice_unstake_request_oc = th
            .execute_unstake_request(
                &HOLDERS,
                &Holder::Alice,
                current_slot,
                &alice_rewarded_staked_oc,
            )
            .await?;

        // We progress after grace period
        current_slot += (calculate_grace_period() * EPOCH_LENGTH) + EPOCH_LENGTH;

        // Now Alice can unstake her owncoin
        th.execute_unstake(&HOLDERS, &Holder::Alice, current_slot, &alice_unstake_request_oc)
            .await?;

        // Statistics
        th.statistics();

        // Thanks for reading
        Ok(())
    })
}
