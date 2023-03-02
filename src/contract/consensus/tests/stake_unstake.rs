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
//! We first airdrop Alica native tokes, and then she can stake and unstake
//! them a couple of times.
//!
//! With this test, we want to confirm the consensus contract state
//! transitions work for a single party and are able to be verified.
//!
//! TODO: Malicious cases

use darkfi::Result;
use darkfi_sdk::crypto::{merkle_prelude::*, poseidon_hash, Coin, MerkleNode, Nullifier};
use log::info;

use darkfi_money_contract::client::{MoneyNote, OwnCoin};

mod harness;
use harness::{init_logger, ConsensusTestHarness};

#[async_std::test]
async fn consensus_contract_stake_unstake() -> Result<()> {
    init_logger();

    const ALICE_AIRDROP: u64 = 1000;

    // Initialize harness
    let mut th = ConsensusTestHarness::new().await?;
    info!(target: "money", "[Faucet] ===================================================");
    info!(target: "money", "[Faucet] Building Money::Transfer params for Alice's airdrop");
    info!(target: "money", "[Faucet] ===================================================");
    let (airdrop_tx, airdrop_params) = th.airdrop_native(ALICE_AIRDROP, th.alice.keypair.public)?;

    info!(target: "money", "[Faucet] ==========================");
    info!(target: "money", "[Faucet] Executing Alice airdrop tx");
    info!(target: "money", "[Faucet] ==========================");
    th.faucet.state.read().await.verify_transactions(&[airdrop_tx.clone()], true).await?;
    th.faucet.merkle_tree.append(&MerkleNode::from(airdrop_params.outputs[0].coin.inner()));
    info!(target: "money", "[Alice] ==========================");
    info!(target: "money", "[Alice] Executing Alice airdrop tx");
    info!(target: "money", "[Alice] ==========================");
    th.alice.state.read().await.verify_transactions(&[airdrop_tx.clone()], true).await?;
    th.alice.merkle_tree.append(&MerkleNode::from(airdrop_params.outputs[0].coin.inner()));

    assert!(th.faucet.merkle_tree.root(0).unwrap() == th.alice.merkle_tree.root(0).unwrap());

    // Gather new owncoins
    let mut owncoins = vec![];
    let leaf_position = th.alice.merkle_tree.witness().unwrap();
    let note: MoneyNote = airdrop_params.outputs[0].note.decrypt(&th.alice.keypair.secret)?;
    owncoins.push(OwnCoin {
        coin: Coin::from(airdrop_params.outputs[0].coin),
        note: note.clone(),
        secret: th.alice.keypair.secret,
        nullifier: Nullifier::from(poseidon_hash([th.alice.keypair.secret.inner(), note.serial])),
        leaf_position,
    });

    // Thanks for reading
    Ok(())
}
