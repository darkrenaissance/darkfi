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

use std::time::{Duration, Instant};

use darkfi::{tx::Transaction, Result};
use darkfi_sdk::{
    crypto::{
        merkle_prelude::*, pallas, pasta_prelude::*, poseidon_hash, MerkleNode, Nullifier,
        ValueBlind, MONEY_CONTRACT_ID,
    },
    ContractCall,
};
use darkfi_serial::{serialize, Encodable};
use log::info;
use rand::rngs::OsRng;

use darkfi_money_contract::{
    client::{swap_v1::SwapCallBuilder, transfer_v1::TransferCallBuilder, MoneyNote, OwnCoin},
    model::{Coin, MoneyTransferParamsV1 as MoneyTransferParams},
    MoneyFunction::{OtcSwapV1 as MoneyOtcSwap, TransferV1 as MoneyTransfer},
    MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};

mod harness;
use harness::{init_logger, MoneyTestHarness};

#[async_std::test]
async fn money_contract_transfer() -> Result<()> {
    init_logger();

    // Some benchmark averages
    let mut swap_sizes = vec![];
    let mut swap_broadcasted_sizes = vec![];
    let mut swap_creation_times = vec![];
    let mut swap_verify_times = vec![];
    let mut transfer_sizes = vec![];
    let mut transfer_broadcasted_sizes = vec![];
    let mut transfer_creation_times = vec![];
    let mut transfer_verify_times = vec![];
    let mut mint_sizes = vec![];
    let mut mint_broadcasted_sizes = vec![];
    let mut mint_creation_times = vec![];
    let mut mint_verify_times = vec![];

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
    let mut th = MoneyTestHarness::new().await?;
    let (mint_pk, mint_zkbin) = th.proving_keys.get(&MONEY_CONTRACT_ZKAS_MINT_NS_V1).unwrap();
    let (burn_pk, burn_zkbin) = th.proving_keys.get(&MONEY_CONTRACT_ZKAS_BURN_NS_V1).unwrap();
    let contract_id = *MONEY_CONTRACT_ID;

    // We're just going to be using a zero spend-hook and user-data
    let rcpt_spend_hook = pallas::Base::zero();
    let rcpt_user_data = pallas::Base::zero();
    let rcpt_user_data_blind = pallas::Base::random(&mut OsRng);

    // TODO: verify this is correct
    let change_spend_hook = pallas::Base::zero();
    let change_user_data = pallas::Base::zero();
    let change_user_data_blind = pallas::Base::random(&mut OsRng);

    let mut alice_owncoins = vec![];
    let mut bob_owncoins = vec![];

    info!(target: "money", "[Alice] ================================");
    info!(target: "money", "[Alice] Building token mint tx for Alice");
    info!(target: "money", "[Alice] ================================");
    let timer = Instant::now();
    let (alice_mint_tx, alice_params) =
        th.mint_token(th.alice.keypair, ALICE_INITIAL, th.alice.keypair.public)?;
    mint_creation_times.push(timer.elapsed());
    let encoded: Vec<u8> = serialize(&alice_mint_tx);
    let size = ::std::mem::size_of_val(&*encoded);
    mint_sizes.push(size);

    info!(target: "money", "[Bob] ==============================");
    info!(target: "money", "[Bob] Building token mint tx for Bob");
    info!(target: "money", "[Bob] ==============================");
    let timer = Instant::now();
    let (bob_mint_tx, bob_params) =
        th.mint_token(th.bob.keypair, BOB_INITIAL, th.bob.keypair.public)?;
    mint_creation_times.push(timer.elapsed());

    // Calculate transaction sizes
    let encoded: Vec<u8> = serialize(&bob_mint_tx);
    let size = ::std::mem::size_of_val(&*encoded);
    mint_sizes.push(size);
    let base58 = bs58::encode(&encoded).into_string();
    let size = ::std::mem::size_of_val(&*base58);
    mint_broadcasted_sizes.push(size);

    info!(target: "money", "[Faucet] =============================");
    info!(target: "money", "[Faucet] Executing Alice token mint tx");
    info!(target: "money", "[Faucet] =============================");
    let timer = Instant::now();
    let erroneous_txs = th
        .faucet
        .state
        .read()
        .await
        .verify_transactions(&[alice_mint_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.faucet.merkle_tree.append(&MerkleNode::from(alice_params.output.coin.inner()));
    mint_verify_times.push(timer.elapsed());

    info!(target: "money", "[Faucet] ===========================");
    info!(target: "money", "[Faucet] Executing Bob token mint tx");
    info!(target: "money", "[Faucet] ===========================");
    let timer = Instant::now();
    let erroneous_txs = th
        .faucet
        .state
        .read()
        .await
        .verify_transactions(&[bob_mint_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.faucet.merkle_tree.append(&MerkleNode::from(bob_params.output.coin.inner()));
    mint_verify_times.push(timer.elapsed());

    info!(target: "money", "[Alice] =============================");
    info!(target: "money", "[Alice] Executing Alice token mint tx");
    info!(target: "money", "[Alice] =============================");
    let timer = Instant::now();
    let erroneous_txs = th
        .alice
        .state
        .read()
        .await
        .verify_transactions(&[alice_mint_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.alice.merkle_tree.append(&MerkleNode::from(alice_params.output.coin.inner()));
    // Alice has to witness this coin because it's hers.
    let alice_leaf_pos = th.alice.merkle_tree.witness().unwrap();
    mint_verify_times.push(timer.elapsed());

    info!(target: "money", "[Alice] ===========================");
    info!(target: "money", "[Alice] Executing Bob token mint tx");
    info!(target: "money", "[Alice] ===========================");
    let timer = Instant::now();
    let erroneous_txs = th
        .alice
        .state
        .read()
        .await
        .verify_transactions(&[bob_mint_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.alice.merkle_tree.append(&MerkleNode::from(bob_params.output.coin.inner()));
    mint_verify_times.push(timer.elapsed());

    info!(target: "money", "[Bob] =============================");
    info!(target: "money", "[Bob] Executing Alice token mint tx");
    info!(target: "money", "[Bob] =============================");
    let timer = Instant::now();
    let erroneous_txs = th
        .bob
        .state
        .read()
        .await
        .verify_transactions(&[alice_mint_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.bob.merkle_tree.append(&MerkleNode::from(alice_params.output.coin.inner()));
    mint_verify_times.push(timer.elapsed());

    info!(target: "money", "[Bob] ===========================");
    info!(target: "money", "[Bob] Executing Bob token mint tx");
    info!(target: "money", "[Bob] ===========================");
    let timer = Instant::now();
    let erroneous_txs = th
        .bob
        .state
        .read()
        .await
        .verify_transactions(&[bob_mint_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.bob.merkle_tree.append(&MerkleNode::from(bob_params.output.coin.inner()));
    let bob_leaf_pos = th.bob.merkle_tree.witness().unwrap();
    mint_verify_times.push(timer.elapsed());

    assert!(th.alice.merkle_tree.root(0).unwrap() == th.bob.merkle_tree.root(0).unwrap());
    assert!(th.faucet.merkle_tree.root(0).unwrap() == th.bob.merkle_tree.root(0).unwrap());

    // Alice builds an `OwnCoin` from her airdrop
    let note: MoneyNote = alice_params.output.note.decrypt(&th.alice.keypair.secret)?;
    let alice_token_id = note.token_id;
    let alice_oc = OwnCoin {
        coin: Coin::from(alice_params.output.coin),
        note: note.clone(),
        secret: th.alice.keypair.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.alice.keypair.secret.inner(), note.serial])),
        leaf_position: alice_leaf_pos,
    };
    alice_owncoins.push(alice_oc);

    // Bob too
    let note: MoneyNote = bob_params.output.note.decrypt(&th.bob.keypair.secret)?;
    let bob_token_id = note.token_id;
    let bob_oc = OwnCoin {
        coin: Coin::from(bob_params.output.coin),
        note: note.clone(),
        secret: th.bob.keypair.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.bob.keypair.secret.inner(), note.serial])),
        leaf_position: bob_leaf_pos,
    };
    bob_owncoins.push(bob_oc);

    // Now Alice can send a little bit of funds to Bob
    info!(target: "money", "[Alice] ====================================================");
    info!(target: "money", "[Alice] Building Money::Transfer params for a payment to Bob");
    info!(target: "money", "[Alice] ====================================================");
    let timer = Instant::now();
    let alice2bob_call_debris = TransferCallBuilder {
        keypair: th.alice.keypair,
        recipient: th.bob.keypair.public,
        value: ALICE_FIRST_SEND,
        token_id: alice_token_id,
        rcpt_spend_hook,
        rcpt_user_data,
        rcpt_user_data_blind,
        change_spend_hook,
        change_user_data,
        change_user_data_blind,
        coins: alice_owncoins.clone(),
        tree: th.alice.merkle_tree.clone(),
        mint_zkbin: mint_zkbin.clone(),
        mint_pk: mint_pk.clone(),
        burn_zkbin: burn_zkbin.clone(),
        burn_pk: burn_pk.clone(),
        clear_input: false,
    }
    .build()?;
    let (alice2bob_params, alice2bob_proofs, alice2bob_secret_keys, alice2bob_spent_coins) = (
        alice2bob_call_debris.params,
        alice2bob_call_debris.proofs,
        alice2bob_call_debris.signature_secrets,
        alice2bob_call_debris.spent_coins,
    );

    assert!(alice2bob_params.inputs.len() == 1);
    assert!(alice2bob_params.outputs.len() == 2);
    assert!(alice2bob_spent_coins.len() == 1);
    alice_owncoins.retain(|x| x != &alice2bob_spent_coins[0]);
    assert!(alice_owncoins.is_empty());

    info!(target: "money", "[Alice] ==========================");
    info!(target: "money", "[Alice] Building payment tx to Bob");
    info!(target: "money", "[Alice] ==========================");
    let mut data = vec![MoneyTransfer as u8];
    alice2bob_params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id, data }];
    let proofs = vec![alice2bob_proofs];
    let mut alice2bob_tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = alice2bob_tx.create_sigs(&mut OsRng, &alice2bob_secret_keys)?;
    alice2bob_tx.signatures = vec![sigs];
    transfer_creation_times.push(timer.elapsed());

    // Calculate transaction sizes
    let encoded: Vec<u8> = serialize(&alice2bob_tx);
    let size = ::std::mem::size_of_val(&*encoded);
    transfer_sizes.push(size);
    let base58 = bs58::encode(&encoded).into_string();
    let size = ::std::mem::size_of_val(&*base58);
    transfer_broadcasted_sizes.push(size);

    info!(target: "money", "[Faucet] ==============================");
    info!(target: "money", "[Faucet] Executing Alice2Bob payment tx");
    info!(target: "money", "[Faucet] ==============================");
    let timer = Instant::now();
    let erroneous_txs = th
        .faucet
        .state
        .read()
        .await
        .verify_transactions(&[alice2bob_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.faucet.merkle_tree.append(&MerkleNode::from(alice2bob_params.outputs[0].coin.inner()));
    th.faucet.merkle_tree.append(&MerkleNode::from(alice2bob_params.outputs[1].coin.inner()));
    transfer_verify_times.push(timer.elapsed());

    info!(target: "money", "[Alice] ==============================");
    info!(target: "money", "[Alice] Executing Alice2Bob payment tx");
    info!(target: "money", "[Alice] ==============================");
    let timer = Instant::now();
    let erroneous_txs = th
        .alice
        .state
        .read()
        .await
        .verify_transactions(&[alice2bob_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.alice.merkle_tree.append(&MerkleNode::from(alice2bob_params.outputs[0].coin.inner()));
    let alice_leaf_pos = th.alice.merkle_tree.witness().unwrap();
    th.alice.merkle_tree.append(&MerkleNode::from(alice2bob_params.outputs[1].coin.inner()));
    transfer_verify_times.push(timer.elapsed());

    info!(target: "money", "[Bob] ==============================");
    info!(target: "money", "[Bob] Executing Alice2Bob payment tx");
    info!(target: "money", "[Bob] ==============================");
    let timer = Instant::now();
    let erroneous_txs = th
        .bob
        .state
        .read()
        .await
        .verify_transactions(&[alice2bob_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.bob.merkle_tree.append(&MerkleNode::from(alice2bob_params.outputs[0].coin.inner()));
    th.bob.merkle_tree.append(&MerkleNode::from(alice2bob_params.outputs[1].coin.inner()));
    let bob_leaf_pos = th.bob.merkle_tree.witness().unwrap();
    transfer_verify_times.push(timer.elapsed());

    assert!(th.alice.merkle_tree.root(0).unwrap() == th.bob.merkle_tree.root(0).unwrap());
    assert!(th.faucet.merkle_tree.root(0).unwrap() == th.bob.merkle_tree.root(0).unwrap());

    // Alice should now have one OwnCoin with the change from the above transaction.
    let note: MoneyNote = alice2bob_params.outputs[0].note.decrypt(&th.alice.keypair.secret)?;
    let alice_oc = OwnCoin {
        coin: Coin::from(alice2bob_params.outputs[0].coin),
        note: note.clone(),
        secret: th.alice.keypair.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.alice.keypair.secret.inner(), note.serial])),
        leaf_position: alice_leaf_pos,
    };
    alice_owncoins.push(alice_oc);

    // Bob should have his old one, and this new one.
    let note: MoneyNote = alice2bob_params.outputs[1].note.decrypt(&th.bob.keypair.secret)?;
    let bob_oc = OwnCoin {
        coin: Coin::from(alice2bob_params.outputs[1].coin),
        note: note.clone(),
        secret: th.bob.keypair.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.bob.keypair.secret.inner(), note.serial])),
        leaf_position: bob_leaf_pos,
    };
    bob_owncoins.push(bob_oc);

    assert!(alice_owncoins.len() == 1);
    assert!(bob_owncoins.len() == 2);

    // Bob can send a little bit to Alice as well
    info!(target: "money", "[Bob] ======================================================");
    info!(target: "money", "[Bob] Building Money::Transfer params for a payment to Alice");
    info!(target: "money", "[Bob] ======================================================");
    let timer = Instant::now();
    let mut bob_owncoins_tmp = bob_owncoins.clone();
    bob_owncoins_tmp.retain(|x| x.note.token_id == bob_token_id);
    let bob2alice_call_debris = TransferCallBuilder {
        keypair: th.bob.keypair,
        recipient: th.alice.keypair.public,
        value: BOB_FIRST_SEND,
        token_id: bob_token_id,
        rcpt_spend_hook,
        rcpt_user_data,
        rcpt_user_data_blind,
        change_spend_hook,
        change_user_data,
        change_user_data_blind,
        coins: bob_owncoins_tmp.clone(),
        tree: th.bob.merkle_tree.clone(),
        mint_zkbin: mint_zkbin.clone(),
        mint_pk: mint_pk.clone(),
        burn_zkbin: burn_zkbin.clone(),
        burn_pk: burn_pk.clone(),
        clear_input: false,
    }
    .build()?;
    let (bob2alice_params, bob2alice_proofs, bob2alice_secret_keys, bob2alice_spent_coins) = (
        bob2alice_call_debris.params,
        bob2alice_call_debris.proofs,
        bob2alice_call_debris.signature_secrets,
        bob2alice_call_debris.spent_coins,
    );

    assert!(bob2alice_params.inputs.len() == 1);
    assert!(bob2alice_params.outputs.len() == 2);
    assert!(bob2alice_spent_coins.len() == 1);
    bob_owncoins.retain(|x| x != &bob2alice_spent_coins[0]);
    assert!(bob_owncoins.len() == 1);

    info!(target: "money", "[Bob] ============================");
    info!(target: "money", "[Bob] Building payment tx to Alice");
    info!(target: "money", "[Bob] ============================");
    let mut data = vec![MoneyTransfer as u8];
    bob2alice_params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id, data }];
    let proofs = vec![bob2alice_proofs];
    let mut bob2alice_tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = bob2alice_tx.create_sigs(&mut OsRng, &bob2alice_secret_keys)?;
    bob2alice_tx.signatures = vec![sigs];
    transfer_creation_times.push(timer.elapsed());

    // Calculate transaction sizes
    let encoded: Vec<u8> = serialize(&bob2alice_tx);
    let size = ::std::mem::size_of_val(&*encoded);
    transfer_sizes.push(size);
    let base58 = bs58::encode(&encoded).into_string();
    let size = ::std::mem::size_of_val(&*base58);
    transfer_broadcasted_sizes.push(size);

    info!(target: "money", "[Faucet] ==============================");
    info!(target: "money", "[Faucet] Executing Bob2Alice payment tx");
    info!(target: "money", "[Faucet] ==============================");
    let timer = Instant::now();
    let erroneous_txs = th
        .faucet
        .state
        .read()
        .await
        .verify_transactions(&[bob2alice_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.faucet.merkle_tree.append(&MerkleNode::from(bob2alice_params.outputs[0].coin.inner()));
    th.faucet.merkle_tree.append(&MerkleNode::from(bob2alice_params.outputs[1].coin.inner()));
    transfer_verify_times.push(timer.elapsed());

    info!(target: "money", "[Alice] ==============================");
    info!(target: "money", "[Alice] Executing Bob2Alice payment tx");
    info!(target: "money", "[Alice] ==============================");
    let timer = Instant::now();
    let erroneous_txs = th
        .alice
        .state
        .read()
        .await
        .verify_transactions(&[bob2alice_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.alice.merkle_tree.append(&MerkleNode::from(bob2alice_params.outputs[0].coin.inner()));
    th.alice.merkle_tree.append(&MerkleNode::from(bob2alice_params.outputs[1].coin.inner()));
    let alice_leaf_pos = th.alice.merkle_tree.witness().unwrap();
    transfer_verify_times.push(timer.elapsed());

    info!(target: "money", "[Bob] ==================+===========");
    info!(target: "money", "[Bob] Executing Bob2Alice payment tx");
    info!(target: "money", "[Bob] ==================+===========");
    let timer = Instant::now();
    let erroneous_txs = th
        .bob
        .state
        .read()
        .await
        .verify_transactions(&[bob2alice_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.bob.merkle_tree.append(&MerkleNode::from(bob2alice_params.outputs[0].coin.inner()));
    let bob_leaf_pos = th.bob.merkle_tree.witness().unwrap();
    th.bob.merkle_tree.append(&MerkleNode::from(bob2alice_params.outputs[1].coin.inner()));
    transfer_verify_times.push(timer.elapsed());

    // Alice should now have two OwnCoins
    let note: MoneyNote = bob2alice_params.outputs[1].note.decrypt(&th.alice.keypair.secret)?;
    let alice_oc = OwnCoin {
        coin: Coin::from(bob2alice_params.outputs[1].coin),
        note: note.clone(),
        secret: th.alice.keypair.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.alice.keypair.secret.inner(), note.serial])),
        leaf_position: alice_leaf_pos,
    };
    alice_owncoins.push(alice_oc);

    // Bob should have two with the change from the above tx
    let note: MoneyNote = bob2alice_params.outputs[0].note.decrypt(&th.bob.keypair.secret)?;
    let bob_oc = OwnCoin {
        coin: Coin::from(bob2alice_params.outputs[0].coin),
        note: note.clone(),
        secret: th.bob.keypair.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.bob.keypair.secret.inner(), note.serial])),
        leaf_position: bob_leaf_pos,
    };
    bob_owncoins.push(bob_oc);

    assert!(alice_owncoins.len() == 2);
    assert!(bob_owncoins.len() == 2);
    assert!(th.alice.merkle_tree.root(0).unwrap() == th.bob.merkle_tree.root(0).unwrap());
    assert!(th.faucet.merkle_tree.root(0).unwrap() == th.bob.merkle_tree.root(0).unwrap());

    assert!(alice_owncoins[0].note.value == ALICE_INITIAL - ALICE_FIRST_SEND);
    assert!(alice_owncoins[0].note.token_id == alice_token_id);
    assert!(alice_owncoins[1].note.value == BOB_FIRST_SEND);
    assert!(alice_owncoins[1].note.token_id == bob_token_id);

    assert!(bob_owncoins[0].note.value == ALICE_FIRST_SEND);
    assert!(bob_owncoins[0].note.token_id == alice_token_id);
    assert!(bob_owncoins[1].note.value == BOB_INITIAL - BOB_FIRST_SEND);
    assert!(bob_owncoins[1].note.token_id == bob_token_id);

    // Alice and Bob decide to swap back their tokens so Alice gets back her initial
    // tokens and Bob gets his.
    info!(target: "money", "[Alice] Building OtcSwap half");
    let timer = Instant::now();
    // Generating  swap blinds
    let value_send_blind = ValueBlind::random(&mut OsRng);
    let value_recv_blind = ValueBlind::random(&mut OsRng);
    let token_send_blind = ValueBlind::random(&mut OsRng);
    let token_recv_blind = ValueBlind::random(&mut OsRng);

    let alice_swap_call_debris = SwapCallBuilder {
        pubkey: th.alice.keypair.public,
        value_send: BOB_FIRST_SEND,
        token_id_send: bob_token_id,
        value_recv: ALICE_FIRST_SEND,
        token_id_recv: alice_token_id,
        user_data_blind_send: rcpt_user_data_blind,
        spend_hook_recv: rcpt_spend_hook,
        user_data_recv: rcpt_user_data,
        value_blinds: [value_send_blind, value_recv_blind],
        token_blinds: [token_send_blind, token_recv_blind],
        coin: alice_owncoins[1].clone(),
        tree: th.alice.merkle_tree.clone(),
        mint_zkbin: mint_zkbin.clone(),
        mint_pk: mint_pk.clone(),
        burn_zkbin: burn_zkbin.clone(),
        burn_pk: burn_pk.clone(),
    }
    .build()?;
    let (alice_swap_params, alice_swap_proofs, alice_signature_secret) = (
        alice_swap_call_debris.params,
        alice_swap_call_debris.proofs,
        alice_swap_call_debris.signature_secret,
    );

    assert!(alice_swap_params.inputs.len() == 1);
    assert!(alice_swap_params.outputs.len() == 1);
    alice_owncoins.remove(1);
    assert!(alice_owncoins.len() == 1);

    // Alice sends Bob necessary data and he builds his half.
    info!(target: "money", "[Bob] Building OtcSwap half");
    let bob_swap_call_debris = SwapCallBuilder {
        pubkey: th.bob.keypair.public,
        value_send: ALICE_FIRST_SEND,
        token_id_send: alice_token_id,
        value_recv: BOB_FIRST_SEND,
        token_id_recv: bob_token_id,
        user_data_blind_send: rcpt_user_data_blind,
        spend_hook_recv: rcpt_spend_hook,
        user_data_recv: rcpt_user_data,
        value_blinds: [value_recv_blind, value_send_blind],
        token_blinds: [token_recv_blind, token_send_blind],
        coin: bob_owncoins[0].clone(),
        tree: th.bob.merkle_tree.clone(),
        mint_zkbin: mint_zkbin.clone(),
        mint_pk: mint_pk.clone(),
        burn_zkbin: burn_zkbin.clone(),
        burn_pk: burn_pk.clone(),
    }
    .build()?;
    let (bob_swap_params, bob_swap_proofs, bob_signature_secret) = (
        bob_swap_call_debris.params,
        bob_swap_call_debris.proofs,
        bob_swap_call_debris.signature_secret,
    );

    assert!(bob_swap_params.inputs.len() == 1);
    assert!(bob_swap_params.outputs.len() == 1);
    bob_owncoins.remove(0);
    assert!(bob_owncoins.len() == 1);

    // Then he combines the halves
    let swap_full_params = MoneyTransferParams {
        clear_inputs: vec![],
        inputs: vec![alice_swap_params.inputs[0].clone(), bob_swap_params.inputs[0].clone()],
        outputs: vec![alice_swap_params.outputs[0].clone(), bob_swap_params.outputs[0].clone()],
    };

    let swap_full_proofs = vec![
        alice_swap_proofs[0].clone(),
        bob_swap_proofs[0].clone(),
        alice_swap_proofs[1].clone(),
        bob_swap_proofs[1].clone(),
    ];

    // And signs the transaction
    let mut data = vec![MoneyOtcSwap as u8];
    swap_full_params.encode(&mut data)?;
    let mut alicebob_swap_tx = Transaction {
        calls: vec![ContractCall { contract_id, data }],
        proofs: vec![swap_full_proofs],
        signatures: vec![],
    };
    let sigs = alicebob_swap_tx.create_sigs(&mut OsRng, &[bob_signature_secret])?;
    alicebob_swap_tx.signatures = vec![sigs];

    // Alice gets the partially signed transaction and adds her signature
    let sigs = alicebob_swap_tx.create_sigs(&mut OsRng, &[alice_signature_secret])?;
    alicebob_swap_tx.signatures[0].insert(0, sigs[0]);
    swap_creation_times.push(timer.elapsed());

    // Calculate transaction sizes
    let encoded: Vec<u8> = serialize(&alicebob_swap_tx);
    let size = ::std::mem::size_of_val(&*encoded);
    swap_sizes.push(size);
    let base58 = bs58::encode(&encoded).into_string();
    let size = ::std::mem::size_of_val(&*base58);
    swap_broadcasted_sizes.push(size);

    info!(target: "money", "[Faucet] ==========================");
    info!(target: "money", "[Faucet] Executing AliceBob swap tx");
    info!(target: "money", "[Faucet] ==========================");
    let timer = Instant::now();
    let erroneous_txs = th
        .faucet
        .state
        .read()
        .await
        .verify_transactions(&[alicebob_swap_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.faucet.merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[0].coin.inner()));
    th.faucet.merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[1].coin.inner()));
    swap_verify_times.push(timer.elapsed());

    info!(target: "money", "[Alice] ==========================");
    info!(target: "money", "[Alice] Executing AliceBob swap tx");
    info!(target: "money", "[Alice] ==========================");
    let timer = Instant::now();
    let erroneous_txs = th
        .alice
        .state
        .read()
        .await
        .verify_transactions(&[alicebob_swap_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.alice.merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[0].coin.inner()));
    let alice_leaf_pos = th.alice.merkle_tree.witness().unwrap();
    th.alice.merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[1].coin.inner()));
    swap_verify_times.push(timer.elapsed());

    info!(target: "money", "[Bob] ==========================");
    info!(target: "money", "[Bob] Executing AliceBob swap tx");
    info!(target: "money", "[Bob] ==========================");
    let timer = Instant::now();
    let erroneous_txs = th
        .bob
        .state
        .read()
        .await
        .verify_transactions(&[alicebob_swap_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.bob.merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[0].coin.inner()));
    th.bob.merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[1].coin.inner()));
    let bob_leaf_pos = th.bob.merkle_tree.witness().unwrap();
    swap_verify_times.push(timer.elapsed());

    assert!(th.alice.merkle_tree.root(0).unwrap() == th.bob.merkle_tree.root(0).unwrap());
    assert!(th.faucet.merkle_tree.root(0).unwrap() == th.bob.merkle_tree.root(0).unwrap());

    // Alice should now have two OwnCoins with the same token ID (ALICE)
    let note: MoneyNote = swap_full_params.outputs[0].note.decrypt(&th.alice.keypair.secret)?;
    let alice_oc = OwnCoin {
        coin: Coin::from(swap_full_params.outputs[0].coin),
        note: note.clone(),
        secret: th.alice.keypair.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.alice.keypair.secret.inner(), note.serial])),
        leaf_position: alice_leaf_pos,
    };
    alice_owncoins.push(alice_oc);

    assert!(alice_owncoins.len() == 2);
    assert!(alice_owncoins[0].note.token_id == alice_token_id);
    assert!(alice_owncoins[1].note.token_id == alice_token_id);

    // Same for Bob with BOB tokens
    let note: MoneyNote = swap_full_params.outputs[1].note.decrypt(&th.bob.keypair.secret)?;
    let bob_oc = OwnCoin {
        coin: Coin::from(swap_full_params.outputs[1].coin),
        note: note.clone(),
        secret: th.bob.keypair.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.bob.keypair.secret.inner(), note.serial])),
        leaf_position: bob_leaf_pos,
    };
    bob_owncoins.push(bob_oc);

    assert!(bob_owncoins.len() == 2);
    assert!(bob_owncoins[0].note.token_id == bob_token_id);
    assert!(bob_owncoins[1].note.token_id == bob_token_id);

    // Now Alice will create a new coin for herself to combine the two owncoins.
    info!(target: "money", "[Alice] ======================================================");
    info!(target: "money", "[Alice] Building Money::Transfer params for a payment to Alice");
    info!(target: "money", "[Alice] ======================================================");
    let timer = Instant::now();
    let alice2alice_call_debris = TransferCallBuilder {
        keypair: th.alice.keypair,
        recipient: th.alice.keypair.public,
        value: ALICE_INITIAL,
        token_id: alice_token_id,
        rcpt_spend_hook,
        rcpt_user_data,
        rcpt_user_data_blind,
        change_spend_hook,
        change_user_data,
        change_user_data_blind,
        coins: alice_owncoins.clone(),
        tree: th.alice.merkle_tree.clone(),
        mint_zkbin: mint_zkbin.clone(),
        mint_pk: mint_pk.clone(),
        burn_zkbin: burn_zkbin.clone(),
        burn_pk: burn_pk.clone(),
        clear_input: false,
    }
    .build()?;
    let (alice2alice_params, alice2alice_proofs, alice2alice_secret_keys, alice2alice_spent_coins) = (
        alice2alice_call_debris.params,
        alice2alice_call_debris.proofs,
        alice2alice_call_debris.signature_secrets,
        alice2alice_call_debris.spent_coins,
    );

    for coin in alice2alice_spent_coins {
        alice_owncoins.retain(|x| x != &coin);
    }
    assert!(alice_owncoins.is_empty());
    assert!(alice2alice_params.inputs.len() == 2);
    assert!(alice2alice_params.outputs.len() == 1);

    info!(target: "money", "[Alice] ============================");
    info!(target: "money", "[Alice] Building payment tx to Alice");
    info!(target: "money", "[Alice] ============================");
    let mut data = vec![MoneyTransfer as u8];
    alice2alice_params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id, data }];
    let proofs = vec![alice2alice_proofs];
    let mut alice2alice_tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = alice2alice_tx.create_sigs(&mut OsRng, &alice2alice_secret_keys)?;
    alice2alice_tx.signatures = vec![sigs];
    transfer_creation_times.push(timer.elapsed());

    // Calculate transaction sizes
    let encoded: Vec<u8> = serialize(&alice2alice_tx);
    let size = ::std::mem::size_of_val(&*encoded);
    transfer_sizes.push(size);
    let base58 = bs58::encode(&encoded).into_string();
    let size = ::std::mem::size_of_val(&*base58);
    transfer_broadcasted_sizes.push(size);

    info!(target: "money", "[Faucet] ================================");
    info!(target: "money", "[Faucet] Executing Alice2Alice payment tx");
    info!(target: "money", "[Faucet] ================================");
    let timer = Instant::now();
    let erroneous_txs = th
        .faucet
        .state
        .read()
        .await
        .verify_transactions(&[alice2alice_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.faucet.merkle_tree.append(&MerkleNode::from(alice2alice_params.outputs[0].coin.inner()));
    transfer_verify_times.push(timer.elapsed());

    info!(target: "money", "[Alice] ================================");
    info!(target: "money", "[Alice] Executing Alice2Alice payment tx");
    info!(target: "money", "[Alice] ================================");
    let timer = Instant::now();
    let erroneous_txs = th
        .alice
        .state
        .read()
        .await
        .verify_transactions(&[alice2alice_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.alice.merkle_tree.append(&MerkleNode::from(alice2alice_params.outputs[0].coin.inner()));
    let alice_leaf_pos = th.alice.merkle_tree.witness().unwrap();
    transfer_verify_times.push(timer.elapsed());

    info!(target: "money", "[Bob] ================================");
    info!(target: "money", "[Bob] Executing Alice2Alice payment tx");
    info!(target: "money", "[Bob] ================================");
    let timer = Instant::now();
    let erroneous_txs = th
        .bob
        .state
        .read()
        .await
        .verify_transactions(&[alice2alice_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.bob.merkle_tree.append(&MerkleNode::from(alice2alice_params.outputs[0].coin.inner()));
    transfer_verify_times.push(timer.elapsed());

    assert!(th.alice.merkle_tree.root(0).unwrap() == th.bob.merkle_tree.root(0).unwrap());
    assert!(th.faucet.merkle_tree.root(0).unwrap() == th.bob.merkle_tree.root(0).unwrap());

    // Alice should now have a single OwnCoin with her initial airdrop
    let note: MoneyNote = alice2alice_params.outputs[0].note.decrypt(&th.alice.keypair.secret)?;
    let alice_oc = OwnCoin {
        coin: Coin::from(alice2alice_params.outputs[0].coin),
        note: note.clone(),
        secret: th.alice.keypair.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.alice.keypair.secret.inner(), note.serial])),
        leaf_position: alice_leaf_pos,
    };
    alice_owncoins.push(alice_oc);

    assert!(alice_owncoins.len() == 1);
    assert!(alice_owncoins[0].note.value == ALICE_INITIAL);
    assert!(alice_owncoins[0].note.token_id == alice_token_id);

    // Bob does the same
    info!(target: "money", "[Bob] ====================================================");
    info!(target: "money", "[Bob] Building Money::Transfer params for a payment to Bob");
    info!(target: "money", "[Bob] ====================================================");
    let timer = Instant::now();
    let bob2bob_call_debris = TransferCallBuilder {
        keypair: th.bob.keypair,
        recipient: th.bob.keypair.public,
        value: BOB_INITIAL,
        token_id: bob_token_id,
        rcpt_spend_hook,
        rcpt_user_data,
        rcpt_user_data_blind,
        change_spend_hook,
        change_user_data,
        change_user_data_blind,
        coins: bob_owncoins.clone(),
        tree: th.bob.merkle_tree.clone(),
        mint_zkbin: mint_zkbin.clone(),
        mint_pk: mint_pk.clone(),
        burn_zkbin: burn_zkbin.clone(),
        burn_pk: burn_pk.clone(),
        clear_input: false,
    }
    .build()?;
    let (bob2bob_params, bob2bob_proofs, bob2bob_secret_keys, bob2bob_spent_coins) = (
        bob2bob_call_debris.params,
        bob2bob_call_debris.proofs,
        bob2bob_call_debris.signature_secrets,
        bob2bob_call_debris.spent_coins,
    );

    for coin in bob2bob_spent_coins {
        bob_owncoins.retain(|x| x != &coin);
    }
    assert!(bob_owncoins.is_empty());
    assert!(bob2bob_params.inputs.len() == 2);
    assert!(bob2bob_params.outputs.len() == 1);

    info!(target: "money", "[Bob] ==========================");
    info!(target: "money", "[Bob] Building payment tx to Bob");
    info!(target: "money", "[Bob] ==========================");
    let mut data = vec![MoneyTransfer as u8];
    bob2bob_params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id, data }];
    let proofs = vec![bob2bob_proofs];
    let mut bob2bob_tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = bob2bob_tx.create_sigs(&mut OsRng, &bob2bob_secret_keys)?;
    bob2bob_tx.signatures = vec![sigs];
    transfer_creation_times.push(timer.elapsed());

    // Calculate transaction sizes
    let encoded: Vec<u8> = serialize(&bob2bob_tx);
    let size = ::std::mem::size_of_val(&*encoded);
    transfer_sizes.push(size);
    let base58 = bs58::encode(&encoded).into_string();
    let size = ::std::mem::size_of_val(&*base58);
    transfer_broadcasted_sizes.push(size);

    info!(target: "money", "[Faucet] ============================");
    info!(target: "money", "[Faucet] Executing Bob2Bob payment tx");
    info!(target: "money", "[Faucet] ============================");
    let timer = Instant::now();
    let erroneous_txs = th
        .faucet
        .state
        .read()
        .await
        .verify_transactions(&[bob2bob_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.faucet.merkle_tree.append(&MerkleNode::from(bob2bob_params.outputs[0].coin.inner()));
    transfer_verify_times.push(timer.elapsed());

    info!(target: "money", "[Alice] ============================");
    info!(target: "money", "[Alice] Executing Bob2Bob payment tx");
    info!(target: "money", "[Alice] ============================");
    let timer = Instant::now();
    let erroneous_txs = th
        .alice
        .state
        .read()
        .await
        .verify_transactions(&[bob2bob_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.alice.merkle_tree.append(&MerkleNode::from(bob2bob_params.outputs[0].coin.inner()));
    transfer_verify_times.push(timer.elapsed());

    info!(target: "money", "[Bob] ============================");
    info!(target: "money", "[Bob] Executing Bob2Bob payment tx");
    info!(target: "money", "[Bob] ============================");
    let timer = Instant::now();
    let erroneous_txs = th
        .bob
        .state
        .read()
        .await
        .verify_transactions(&[bob2bob_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.bob.merkle_tree.append(&MerkleNode::from(bob2bob_params.outputs[0].coin.inner()));
    let bob_leaf_pos = th.bob.merkle_tree.witness().unwrap();
    transfer_verify_times.push(timer.elapsed());

    assert!(th.alice.merkle_tree.root(0).unwrap() == th.bob.merkle_tree.root(0).unwrap());
    assert!(th.faucet.merkle_tree.root(0).unwrap() == th.bob.merkle_tree.root(0).unwrap());

    // Bob should now have a single OwnCoin with her initial airdrop
    let note: MoneyNote = bob2bob_params.outputs[0].note.decrypt(&th.bob.keypair.secret)?;
    let bob_oc = OwnCoin {
        coin: Coin::from(bob2bob_params.outputs[0].coin),
        note: note.clone(),
        secret: th.bob.keypair.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.bob.keypair.secret.inner(), note.serial])),
        leaf_position: bob_leaf_pos,
    };
    bob_owncoins.push(bob_oc);

    assert!(bob_owncoins.len() == 1);
    assert!(bob_owncoins[0].note.value == BOB_INITIAL);
    assert!(bob_owncoins[0].note.token_id == bob_token_id);

    // Now they decide to swap all of their tokens
    info!(target: "money", "[Alice] Building OtcSwap half");
    let timer = Instant::now();
    // Generating  swap blinds
    let value_send_blind = ValueBlind::random(&mut OsRng);
    let value_recv_blind = ValueBlind::random(&mut OsRng);
    let token_send_blind = ValueBlind::random(&mut OsRng);
    let token_recv_blind = ValueBlind::random(&mut OsRng);

    let alice_swap_call_debris = SwapCallBuilder {
        pubkey: th.alice.keypair.public,
        value_send: ALICE_INITIAL,
        token_id_send: alice_token_id,
        value_recv: BOB_INITIAL,
        token_id_recv: bob_token_id,
        user_data_blind_send: rcpt_user_data_blind,
        spend_hook_recv: rcpt_spend_hook,
        user_data_recv: rcpt_user_data,
        value_blinds: [value_send_blind, value_recv_blind],
        token_blinds: [token_send_blind, token_recv_blind],
        coin: alice_owncoins[0].clone(),
        tree: th.alice.merkle_tree.clone(),
        mint_zkbin: mint_zkbin.clone(),
        mint_pk: mint_pk.clone(),
        burn_zkbin: burn_zkbin.clone(),
        burn_pk: burn_pk.clone(),
    }
    .build()?;
    let (alice_swap_params, alice_swap_proofs, alice_signature_secret) = (
        alice_swap_call_debris.params,
        alice_swap_call_debris.proofs,
        alice_swap_call_debris.signature_secret,
    );

    assert!(alice_swap_params.inputs.len() == 1);
    assert!(alice_swap_params.outputs.len() == 1);
    alice_owncoins.remove(0);
    assert!(alice_owncoins.is_empty());

    info!(target: "money", "[Bob] Building OtcSwap half");
    let bob_swap_call_debris = SwapCallBuilder {
        pubkey: th.bob.keypair.public,
        value_send: BOB_INITIAL,
        token_id_send: bob_token_id,
        value_recv: ALICE_INITIAL,
        token_id_recv: alice_token_id,
        user_data_blind_send: rcpt_user_data_blind,
        spend_hook_recv: rcpt_spend_hook,
        user_data_recv: rcpt_user_data,
        value_blinds: [value_recv_blind, value_send_blind],
        token_blinds: [token_recv_blind, token_send_blind],
        coin: bob_owncoins[0].clone(),
        tree: th.bob.merkle_tree.clone(),
        mint_zkbin: mint_zkbin.clone(),
        mint_pk: mint_pk.clone(),
        burn_zkbin: burn_zkbin.clone(),
        burn_pk: burn_pk.clone(),
    }
    .build()?;
    let (bob_swap_params, bob_swap_proofs, bob_signature_secret) = (
        bob_swap_call_debris.params,
        bob_swap_call_debris.proofs,
        bob_swap_call_debris.signature_secret,
    );

    assert!(bob_swap_params.inputs.len() == 1);
    assert!(bob_swap_params.outputs.len() == 1);
    bob_owncoins.remove(0);
    assert!(bob_owncoins.is_empty());

    let swap_full_params = MoneyTransferParams {
        clear_inputs: vec![],
        inputs: vec![alice_swap_params.inputs[0].clone(), bob_swap_params.inputs[0].clone()],
        outputs: vec![alice_swap_params.outputs[0].clone(), bob_swap_params.outputs[0].clone()],
    };

    let swap_full_proofs = vec![
        alice_swap_proofs[0].clone(),
        bob_swap_proofs[0].clone(),
        alice_swap_proofs[1].clone(),
        bob_swap_proofs[1].clone(),
    ];

    // And signs the transaction
    let mut data = vec![MoneyOtcSwap as u8];
    swap_full_params.encode(&mut data)?;
    let mut alicebob_swap_tx = Transaction {
        calls: vec![ContractCall { contract_id, data }],
        proofs: vec![swap_full_proofs],
        signatures: vec![],
    };
    let sigs = alicebob_swap_tx.create_sigs(&mut OsRng, &[bob_signature_secret])?;
    alicebob_swap_tx.signatures = vec![sigs];

    // Alice gets the partially signed transaction and adds her signature
    let sigs = alicebob_swap_tx.create_sigs(&mut OsRng, &[alice_signature_secret])?;
    alicebob_swap_tx.signatures[0].insert(0, sigs[0]);
    swap_creation_times.push(timer.elapsed());

    // Calculate transaction sizes
    let encoded: Vec<u8> = serialize(&alicebob_swap_tx);
    let size = ::std::mem::size_of_val(&*encoded);
    swap_sizes.push(size);
    let base58 = bs58::encode(&encoded).into_string();
    let size = ::std::mem::size_of_val(&*base58);
    swap_broadcasted_sizes.push(size);

    info!(target: "money", "[Faucet] ==========================");
    info!(target: "money", "[Faucet] Executing AliceBob swap tx");
    info!(target: "money", "[Faucet] ==========================");
    let timer = Instant::now();
    let erroneous_txs = th
        .faucet
        .state
        .read()
        .await
        .verify_transactions(&[alicebob_swap_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.faucet.merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[0].coin.inner()));
    th.faucet.merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[1].coin.inner()));
    swap_verify_times.push(timer.elapsed());

    info!(target: "money", "[Alice] ==========================");
    info!(target: "money", "[Alice] Executing AliceBob swap tx");
    info!(target: "money", "[Alice] ==========================");
    let timer = Instant::now();
    let erroneous_txs = th
        .alice
        .state
        .read()
        .await
        .verify_transactions(&[alicebob_swap_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.alice.merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[0].coin.inner()));
    let alice_leaf_pos = th.alice.merkle_tree.witness().unwrap();
    th.alice.merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[1].coin.inner()));
    swap_verify_times.push(timer.elapsed());

    info!(target: "money", "[Bob] ==========================");
    info!(target: "money", "[Bob] Executing AliceBob swap tx");
    info!(target: "money", "[Bob] ==========================");
    let timer = Instant::now();
    let erroneous_txs = th
        .bob
        .state
        .read()
        .await
        .verify_transactions(&[alicebob_swap_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.bob.merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[0].coin.inner()));
    th.bob.merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[1].coin.inner()));
    let bob_leaf_pos = th.bob.merkle_tree.witness().unwrap();
    swap_verify_times.push(timer.elapsed());

    assert!(th.alice.merkle_tree.root(0).unwrap() == th.bob.merkle_tree.root(0).unwrap());
    assert!(th.faucet.merkle_tree.root(0).unwrap() == th.bob.merkle_tree.root(0).unwrap());

    // Alice should now have Bob's BOB tokens
    let note: MoneyNote = swap_full_params.outputs[0].note.decrypt(&th.alice.keypair.secret)?;
    let alice_oc = OwnCoin {
        coin: Coin::from(swap_full_params.outputs[0].coin),
        note: note.clone(),
        secret: th.alice.keypair.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.alice.keypair.secret.inner(), note.serial])),
        leaf_position: alice_leaf_pos,
    };
    alice_owncoins.push(alice_oc);

    assert!(alice_owncoins.len() == 1);
    assert!(alice_owncoins[0].note.value == BOB_INITIAL);
    assert!(alice_owncoins[0].note.token_id == bob_token_id);

    // And Bob should have Alice's ALICE tokens
    let note: MoneyNote = swap_full_params.outputs[1].note.decrypt(&th.bob.keypair.secret)?;
    let bob_oc = OwnCoin {
        coin: Coin::from(swap_full_params.outputs[1].coin),
        note: note.clone(),
        secret: th.bob.keypair.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.bob.keypair.secret.inner(), note.serial])),
        leaf_position: bob_leaf_pos,
    };
    bob_owncoins.push(bob_oc);

    assert!(bob_owncoins.len() == 1);
    assert!(bob_owncoins[0].note.value == ALICE_INITIAL);
    assert!(bob_owncoins[0].note.token_id == alice_token_id);

    // Statistics
    let swap_avg = swap_sizes.iter().sum::<usize>();
    let swap_avg = swap_avg / swap_sizes.len();
    info!("Average Swap size: {:?} Bytes", swap_avg);
    let swap_avg = swap_broadcasted_sizes.iter().sum::<usize>();
    let swap_avg = swap_avg / swap_broadcasted_sizes.len();
    info!("Average Swap broadcasted size: {:?} Bytes", swap_avg);
    let swap_avg = swap_creation_times.iter().sum::<Duration>();
    let swap_avg = swap_avg / swap_creation_times.len() as u32;
    info!("Average Swap creation time: {:?}", swap_avg);
    let swap_avg = swap_verify_times.iter().sum::<Duration>();
    let swap_avg = swap_avg / swap_verify_times.len() as u32;
    info!("Average Swap verification time: {:?}", swap_avg);

    let transfer_avg = transfer_sizes.iter().sum::<usize>();
    let transfer_avg = transfer_avg / transfer_sizes.len();
    info!("Average Transfer size: {:?} Bytes", transfer_avg);
    let transfer_avg = transfer_broadcasted_sizes.iter().sum::<usize>();
    let transfer_avg = transfer_avg / transfer_broadcasted_sizes.len();
    info!("Average Transfer broadcasted size: {:?} Bytes", transfer_avg);
    let transfer_avg = transfer_creation_times.iter().sum::<Duration>();
    let transfer_avg = transfer_avg / transfer_creation_times.len() as u32;
    info!("Average Transfer creation time: {:?}", transfer_avg);
    let transfer_avg = transfer_verify_times.iter().sum::<Duration>();
    let transfer_avg = transfer_avg / transfer_verify_times.len() as u32;
    info!("Average Transfer verification time: {:?}", transfer_avg);

    let mint_avg = mint_sizes.iter().sum::<usize>();
    let mint_avg = mint_avg / mint_sizes.len();
    info!("Average Mint size: {:?} Bytes", mint_avg);
    let mint_avg = mint_broadcasted_sizes.iter().sum::<usize>();
    let mint_avg = mint_avg / mint_broadcasted_sizes.len();
    info!("Average Mint broadcasted size: {:?} Bytes", mint_avg);
    let mint_avg = mint_creation_times.iter().sum::<Duration>();
    let mint_avg = mint_avg / mint_creation_times.len() as u32;
    info!("Average Mint creation time: {:?}", mint_avg);
    let mint_avg = mint_verify_times.iter().sum::<Duration>();
    let mint_avg = mint_avg / mint_verify_times.len() as u32;
    info!("Average Mint verification time: {:?}", mint_avg);

    // Thanks for reading
    Ok(())
}
