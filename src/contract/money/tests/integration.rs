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

//! Integration test for functionalities of the money smart contract:
//!
//! * Airdrops of the native token from the faucet
//! * Arbitrary token minting
//! * Transfers/Payments
//! * Atomic swaps
//! * Token mint freezing
//!
//! With this test we want to confirm the money contract state transitions
//! work between multiple parties and are able to be verified.
//!
//! TODO: Malicious cases

use darkfi::Result;
use darkfi_sdk::{
    crypto::{poseidon_hash, Keypair, MerkleNode, Nullifier},
    incrementalmerkletree::Tree,
};
use log::info;
use rand::rngs::OsRng;

use darkfi_money_contract::client::{MoneyNote, OwnCoin};

mod harness;
use harness::{init_logger, MoneyTestHarness};

#[async_std::test]
async fn money_integration() -> Result<()> {
    init_logger();

    // Slot to verify against
    let current_slot = 0;

    let mut th = MoneyTestHarness::new().await?;

    // Let's first airdrop some tokens to Alice.
    let (alice_airdrop_tx, alice_airdrop_params) =
        th.airdrop_native(200, th.alice.keypair.public)?;

    info!("[Faucet] Executing Alice airdrop tx");
    th.faucet
        .state
        .read()
        .await
        .verify_transactions(&[alice_airdrop_tx.clone()], current_slot, true)
        .await?;
    th.faucet.merkle_tree.append(&MerkleNode::from(alice_airdrop_params.outputs[0].coin.inner()));

    info!("[Alice] Executing Alice airdrop tx");
    th.alice
        .state
        .read()
        .await
        .verify_transactions(&[alice_airdrop_tx.clone()], current_slot, true)
        .await?;
    th.alice.merkle_tree.append(&MerkleNode::from(alice_airdrop_params.outputs[0].coin.inner()));
    // Alice has to witness this coin because it's hers.
    let leaf_position = th.alice.merkle_tree.witness().unwrap();

    info!("[Bob] Executing Alice airdrop tx");
    th.bob
        .state
        .read()
        .await
        .verify_transactions(&[alice_airdrop_tx.clone()], current_slot, true)
        .await?;
    th.bob.merkle_tree.append(&MerkleNode::from(alice_airdrop_params.outputs[0].coin.inner()));

    info!("[Charlie] Executing Alice airdrop tx");
    th.charlie
        .state
        .read()
        .await
        .verify_transactions(&[alice_airdrop_tx.clone()], current_slot, true)
        .await?;
    th.charlie.merkle_tree.append(&MerkleNode::from(alice_airdrop_params.outputs[0].coin.inner()));

    assert_eq!(th.alice.merkle_tree.root(0).unwrap(), th.bob.merkle_tree.root(0).unwrap());
    assert_eq!(th.bob.merkle_tree.root(0).unwrap(), th.charlie.merkle_tree.root(0).unwrap());
    assert_eq!(th.faucet.merkle_tree.root(0).unwrap(), th.charlie.merkle_tree.root(0).unwrap());

    // Alice builds an `OwnCoin` from her airdrop.
    let note: MoneyNote = alice_airdrop_params.outputs[0].note.decrypt(&th.alice.keypair.secret)?;
    let owncoin = OwnCoin {
        coin: alice_airdrop_params.outputs[0].coin,
        note: note.clone(),
        secret: th.alice.keypair.secret,
        nullifier: Nullifier::from(poseidon_hash([th.alice.keypair.secret.inner(), note.serial])),
        leaf_position,
    };
    th.alice.coins.push(owncoin);

    // Bob creates a new mint authority keypair and mints some tokens for Charlie.
    let bob_token_authority = Keypair::random(&mut OsRng);
    let (bob_charlie_mint_tx, bob_charlie_mint_params) =
        th.mint_token(bob_token_authority, 500, th.charlie.keypair.public)?;

    info!("[Faucet] Executing BOBTOKEN mint to Charlie");
    th.faucet
        .state
        .read()
        .await
        .verify_transactions(&[bob_charlie_mint_tx.clone()], current_slot, true)
        .await?;
    th.faucet.merkle_tree.append(&MerkleNode::from(bob_charlie_mint_params.output.coin.inner()));

    info!("[Alice] Executing BOBTOKEN mint to Charlie");
    th.alice
        .state
        .read()
        .await
        .verify_transactions(&[bob_charlie_mint_tx.clone()], current_slot, true)
        .await?;
    th.alice.merkle_tree.append(&MerkleNode::from(bob_charlie_mint_params.output.coin.inner()));

    info!("[Bob] Executing BOBTOKEN mint to Charlie");
    th.bob
        .state
        .read()
        .await
        .verify_transactions(&[bob_charlie_mint_tx.clone()], current_slot, true)
        .await?;
    th.bob.merkle_tree.append(&MerkleNode::from(bob_charlie_mint_params.output.coin.inner()));

    info!("[Charlie] Executing BOBTOKEN mint to Charlie");
    th.charlie
        .state
        .read()
        .await
        .verify_transactions(&[bob_charlie_mint_tx.clone()], current_slot, true)
        .await?;
    th.charlie.merkle_tree.append(&MerkleNode::from(bob_charlie_mint_params.output.coin.inner()));
    // Charlie has to witness this coin because it's his.
    let leaf_position = th.charlie.merkle_tree.witness().unwrap();

    assert_eq!(th.alice.merkle_tree.root(0).unwrap(), th.bob.merkle_tree.root(0).unwrap());
    assert_eq!(th.bob.merkle_tree.root(0).unwrap(), th.charlie.merkle_tree.root(0).unwrap());
    assert_eq!(th.faucet.merkle_tree.root(0).unwrap(), th.charlie.merkle_tree.root(0).unwrap());

    // Charlie builds an `OwnCoin` from this mint.
    let note: MoneyNote =
        bob_charlie_mint_params.output.note.decrypt(&th.charlie.keypair.secret)?;

    let owncoin = OwnCoin {
        coin: bob_charlie_mint_params.output.coin,
        note: note.clone(),
        secret: th.charlie.keypair.secret,
        nullifier: Nullifier::from(poseidon_hash([th.charlie.keypair.secret.inner(), note.serial])),
        leaf_position,
    };
    th.charlie.coins.push(owncoin);

    // Let's attempt to freeze the BOBTOKEN mint,
    // and after that we shouldn't be able to mint anymore.
    let (bob_frz_tx, _) = th.freeze_token(bob_token_authority)?;

    info!("[Faucet] Executing BOBTOKEN freeze");
    th.faucet
        .state
        .read()
        .await
        .verify_transactions(&[bob_frz_tx.clone()], current_slot, true)
        .await?;

    info!("[Alice] Executing BOBTOKEN freeze");
    th.alice
        .state
        .read()
        .await
        .verify_transactions(&[bob_frz_tx.clone()], current_slot, true)
        .await?;

    info!("[Bob] Executing BOBTOKEN freeze");
    th.bob
        .state
        .read()
        .await
        .verify_transactions(&[bob_frz_tx.clone()], current_slot, true)
        .await?;

    info!("[Charlie] Executing BOBTOKEN freeze");
    th.charlie
        .state
        .read()
        .await
        .verify_transactions(&[bob_frz_tx.clone()], current_slot, true)
        .await?;

    // Thanks for reading
    Ok(())
}
