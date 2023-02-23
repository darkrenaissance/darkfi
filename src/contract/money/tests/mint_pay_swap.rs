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

use darkfi::{tx::Transaction, Result};
use darkfi_sdk::{
    crypto::{
        merkle_prelude::*, pallas, pasta_prelude::*, poseidon_hash, Coin, MerkleNode, Nullifier,
        ValueBlind,
    },
    ContractCall,
};
use darkfi_serial::Encodable;
use log::info;
use rand::rngs::OsRng;

use darkfi_money_contract::{
    client::{
        mint_v1::MintCallBuilder, swap_v1::SwapCallBuilder, transfer_v1::TransferCallBuilder,
        MoneyNote, OwnCoin,
    },
    model::MoneyTransferParamsV1 as MoneyTransferParams,
    MoneyFunction::{MintV1 as MoneyMint, OtcSwapV1 as MoneyOtcSwap, TransferV1 as MoneyTransfer},
};

mod harness;
use harness::{init_logger, MoneyTestHarness};

#[async_std::test]
async fn money_contract_transfer() -> Result<()> {
    init_logger()?;

    // Some numbers we want to assert
    const ALICE_INITIAL: u64 = 100;
    const BOB_INITIAL: u64 = 200;

    // Alice = 50 ALICE
    // Bob = 200 BOB + 50 ALICE
    const ALICE_FIRST_SEND: u64 = ALICE_INITIAL - 50;
    // Alice = 50 ALICE + 180 BOB
    // Bob = 20 BOB + 50 ALICE
    const BOB_FIRST_SEND: u64 = BOB_INITIAL - 20;

    let mut th = MoneyTestHarness::new().await?;

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

    info!(target: "money", "[Alice] ==================================================");
    info!(target: "money", "[Alice] Building Money::Mint params for Alice's token mint");
    info!(target: "money", "[Alice] ==================================================");
    let alice_call_debris = MintCallBuilder {
        mint_authority: th.alice_kp,
        recipient: th.alice_kp.public,
        amount: ALICE_INITIAL,
        spend_hook: rcpt_spend_hook,
        user_data: rcpt_user_data,
        token_mint_zkbin: th.token_mint_zkbin.clone(),
        token_mint_pk: th.token_mint_pk.clone(),
    }
    .build()?;
    let (alice_params, alice_proofs) = (alice_call_debris.params, alice_call_debris.proofs);

    info!(target: "money", "[Bob] ================================================");
    info!(target: "money", "[Bob] Building Money::Mint params for Bob's token mint");
    info!(target: "money", "[Bob] ================================================");
    let bob_call_debris = MintCallBuilder {
        mint_authority: th.bob_kp,
        recipient: th.bob_kp.public,
        amount: BOB_INITIAL,
        spend_hook: rcpt_spend_hook,
        user_data: rcpt_user_data,
        token_mint_zkbin: th.token_mint_zkbin.clone(),
        token_mint_pk: th.token_mint_pk.clone(),
    }
    .build()?;
    let (bob_params, bob_proofs) = (bob_call_debris.params, bob_call_debris.proofs);

    info!(target: "money", "[Alice] ========================================");
    info!(target: "money", "[Alice] Building token mint tx with Alice params");
    info!(target: "money", "[Alice] ========================================");
    let mut data = vec![MoneyMint as u8];
    alice_params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id: th.money_contract_id, data }];
    let proofs = vec![alice_proofs];
    let mut alice_mint_tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = alice_mint_tx.create_sigs(&mut OsRng, &[th.alice_kp.secret])?;
    alice_mint_tx.signatures = vec![sigs];

    info!(target: "money", "[Bob] ======================================");
    info!(target: "money", "[Bob] Building token mint tx with Bob params");
    info!(target: "money", "[Bob] ======================================");
    let mut data = vec![MoneyMint as u8];
    bob_params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id: th.money_contract_id, data }];
    let proofs = vec![bob_proofs];
    let mut bob_mint_tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = bob_mint_tx.create_sigs(&mut OsRng, &[th.bob_kp.secret])?;
    bob_mint_tx.signatures = vec![sigs];

    info!(target: "money", "[Faucet] =============================");
    info!(target: "money", "[Faucet] Executing Alice token mint tx");
    info!(target: "money", "[Faucet] =============================");
    th.faucet_state.read().await.verify_transactions(&[alice_mint_tx.clone()], true).await?;
    th.faucet_merkle_tree.append(&MerkleNode::from(alice_params.output.coin.inner()));

    info!(target: "money", "[Faucet] ===========================");
    info!(target: "money", "[Faucet] Executing Bob token mint tx");
    info!(target: "money", "[Faucet] ===========================");
    th.faucet_state.read().await.verify_transactions(&[bob_mint_tx.clone()], true).await?;
    th.faucet_merkle_tree.append(&MerkleNode::from(bob_params.output.coin.inner()));

    info!(target: "money", "[Alice] =============================");
    info!(target: "money", "[Alice] Executing Alice token mint tx");
    info!(target: "money", "[Alice] =============================");
    th.alice_state.read().await.verify_transactions(&[alice_mint_tx.clone()], true).await?;
    th.alice_merkle_tree.append(&MerkleNode::from(alice_params.output.coin.inner()));
    // Alice has to witness this coin because it's hers.
    let alice_leaf_pos = th.alice_merkle_tree.witness().unwrap();

    info!(target: "money", "[Alice] ===========================");
    info!(target: "money", "[Alice] Executing Bob token mint tx");
    info!(target: "money", "[Alice] ===========================");
    th.alice_state.read().await.verify_transactions(&[bob_mint_tx.clone()], true).await?;
    th.alice_merkle_tree.append(&MerkleNode::from(bob_params.output.coin.inner()));

    info!(target: "money", "[Bob] =============================");
    info!(target: "money", "[Bob] Executing Alice token mint tx");
    info!(target: "money", "[Bob] =============================");
    th.bob_state.read().await.verify_transactions(&[alice_mint_tx.clone()], true).await?;
    th.bob_merkle_tree.append(&MerkleNode::from(alice_params.output.coin.inner()));

    info!(target: "money", "[Bob] ===========================");
    info!(target: "money", "[Bob] Executing Bob token mint tx");
    info!(target: "money", "[Bob] ===========================");
    th.bob_state.read().await.verify_transactions(&[bob_mint_tx.clone()], true).await?;
    th.bob_merkle_tree.append(&MerkleNode::from(bob_params.output.coin.inner()));
    let bob_leaf_pos = th.bob_merkle_tree.witness().unwrap();

    assert!(th.alice_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());
    assert!(th.faucet_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());

    // Alice builds an `OwnCoin` from her airdrop
    let note: MoneyNote = alice_params.output.note.decrypt(&th.alice_kp.secret)?;
    let alice_token_id = note.token_id;
    let alice_oc = OwnCoin {
        coin: Coin::from(alice_params.output.coin),
        note: note.clone(),
        secret: th.alice_kp.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.alice_kp.secret.inner(), note.serial])),
        leaf_position: alice_leaf_pos,
    };
    alice_owncoins.push(alice_oc);

    // Bob too
    let note: MoneyNote = bob_params.output.note.decrypt(&th.bob_kp.secret)?;
    let bob_token_id = note.token_id;
    let bob_oc = OwnCoin {
        coin: Coin::from(bob_params.output.coin),
        note: note.clone(),
        secret: th.bob_kp.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.bob_kp.secret.inner(), note.serial])),
        leaf_position: bob_leaf_pos,
    };
    bob_owncoins.push(bob_oc);

    // Now Alice can send a little bit of funds to Bob
    info!(target: "money", "[Alice] ====================================================");
    info!(target: "money", "[Alice] Building Money::Transfer params for a payment to Bob");
    info!(target: "money", "[Alice] ====================================================");
    let alice2bob_call_debris = TransferCallBuilder {
        keypair: th.alice_kp,
        recipient: th.bob_kp.public,
        value: ALICE_FIRST_SEND,
        token_id: alice_token_id,
        rcpt_spend_hook,
        rcpt_user_data,
        rcpt_user_data_blind,
        change_spend_hook,
        change_user_data,
        change_user_data_blind,
        coins: alice_owncoins.clone(),
        tree: th.alice_merkle_tree.clone(),
        mint_zkbin: th.mint_zkbin.clone(),
        mint_pk: th.mint_pk.clone(),
        burn_zkbin: th.burn_zkbin.clone(),
        burn_pk: th.burn_pk.clone(),
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
    let calls = vec![ContractCall { contract_id: th.money_contract_id, data }];
    let proofs = vec![alice2bob_proofs];
    let mut alice2bob_tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = alice2bob_tx.create_sigs(&mut OsRng, &alice2bob_secret_keys)?;
    alice2bob_tx.signatures = vec![sigs];

    info!(target: "money", "[Faucet] ==============================");
    info!(target: "money", "[Faucet] Executing Alice2Bob payment tx");
    info!(target: "money", "[Faucet] ==============================");
    th.faucet_state.read().await.verify_transactions(&[alice2bob_tx.clone()], true).await?;
    th.faucet_merkle_tree.append(&MerkleNode::from(alice2bob_params.outputs[0].coin.inner()));
    th.faucet_merkle_tree.append(&MerkleNode::from(alice2bob_params.outputs[1].coin.inner()));

    info!(target: "money", "[Alice] ==============================");
    info!(target: "money", "[Alice] Executing Alice2Bob payment tx");
    info!(target: "money", "[Alice] ==============================");
    th.alice_state.read().await.verify_transactions(&[alice2bob_tx.clone()], true).await?;
    th.alice_merkle_tree.append(&MerkleNode::from(alice2bob_params.outputs[0].coin.inner()));
    let alice_leaf_pos = th.alice_merkle_tree.witness().unwrap();
    th.alice_merkle_tree.append(&MerkleNode::from(alice2bob_params.outputs[1].coin.inner()));

    info!(target: "money", "[Bob] ==============================");
    info!(target: "money", "[Bob] Executing Alice2Bob payment tx");
    info!(target: "money", "[Bob] ==============================");
    th.bob_state.read().await.verify_transactions(&[alice2bob_tx.clone()], true).await?;
    th.bob_merkle_tree.append(&MerkleNode::from(alice2bob_params.outputs[0].coin.inner()));
    th.bob_merkle_tree.append(&MerkleNode::from(alice2bob_params.outputs[1].coin.inner()));
    let bob_leaf_pos = th.bob_merkle_tree.witness().unwrap();

    assert!(th.alice_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());
    assert!(th.faucet_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());

    // Alice should now have one OwnCoin with the change from the above transaction.
    let note: MoneyNote = alice2bob_params.outputs[0].note.decrypt(&th.alice_kp.secret)?;
    let alice_oc = OwnCoin {
        coin: Coin::from(alice2bob_params.outputs[0].coin),
        note: note.clone(),
        secret: th.alice_kp.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.alice_kp.secret.inner(), note.serial])),
        leaf_position: alice_leaf_pos,
    };
    alice_owncoins.push(alice_oc);

    // Bob should have his old one, and this new one.
    let note: MoneyNote = alice2bob_params.outputs[1].note.decrypt(&th.bob_kp.secret)?;
    let bob_oc = OwnCoin {
        coin: Coin::from(alice2bob_params.outputs[1].coin),
        note: note.clone(),
        secret: th.bob_kp.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.bob_kp.secret.inner(), note.serial])),
        leaf_position: bob_leaf_pos,
    };
    bob_owncoins.push(bob_oc);

    assert!(alice_owncoins.len() == 1);
    assert!(bob_owncoins.len() == 2);

    // Bob can send a little bit to Alice as well
    info!(target: "money", "[Bob] ======================================================");
    info!(target: "money", "[Bob] Building Money::Transfer params for a payment to Alice");
    info!(target: "money", "[Bob] ======================================================");
    let mut bob_owncoins_tmp = bob_owncoins.clone();
    bob_owncoins_tmp.retain(|x| x.note.token_id == bob_token_id);
    let bob2alice_call_debris = TransferCallBuilder {
        keypair: th.bob_kp,
        recipient: th.alice_kp.public,
        value: BOB_FIRST_SEND,
        token_id: bob_token_id,
        rcpt_spend_hook,
        rcpt_user_data,
        rcpt_user_data_blind,
        change_spend_hook,
        change_user_data,
        change_user_data_blind,
        coins: bob_owncoins.clone(),
        tree: th.bob_merkle_tree.clone(),
        mint_zkbin: th.mint_zkbin.clone(),
        mint_pk: th.mint_pk.clone(),
        burn_zkbin: th.burn_zkbin.clone(),
        burn_pk: th.burn_pk.clone(),
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
    let calls = vec![ContractCall { contract_id: th.money_contract_id, data }];
    let proofs = vec![bob2alice_proofs];
    let mut bob2alice_tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = bob2alice_tx.create_sigs(&mut OsRng, &bob2alice_secret_keys)?;
    bob2alice_tx.signatures = vec![sigs];

    info!(target: "money", "[Faucet] ==============================");
    info!(target: "money", "[Faucet] Executing Bob2Alice payment tx");
    info!(target: "money", "[Faucet] ==============================");
    th.faucet_state.read().await.verify_transactions(&[bob2alice_tx.clone()], true).await?;
    th.faucet_merkle_tree.append(&MerkleNode::from(bob2alice_params.outputs[0].coin.inner()));
    th.faucet_merkle_tree.append(&MerkleNode::from(bob2alice_params.outputs[1].coin.inner()));

    info!(target: "money", "[Alice] ==============================");
    info!(target: "money", "[Alice] Executing Bob2Alice payment tx");
    info!(target: "money", "[Alice] ==============================");
    th.alice_state.read().await.verify_transactions(&[bob2alice_tx.clone()], true).await?;
    th.alice_merkle_tree.append(&MerkleNode::from(bob2alice_params.outputs[0].coin.inner()));
    th.alice_merkle_tree.append(&MerkleNode::from(bob2alice_params.outputs[1].coin.inner()));
    let alice_leaf_pos = th.alice_merkle_tree.witness().unwrap();

    info!(target: "money", "[Bob] ==================+===========");
    info!(target: "money", "[Bob] Executing Bob2Alice payment tx");
    info!(target: "money", "[Bob] ==================+===========");
    th.bob_state.read().await.verify_transactions(&[bob2alice_tx.clone()], true).await?;
    th.bob_merkle_tree.append(&MerkleNode::from(bob2alice_params.outputs[0].coin.inner()));
    let bob_leaf_pos = th.bob_merkle_tree.witness().unwrap();
    th.bob_merkle_tree.append(&MerkleNode::from(bob2alice_params.outputs[1].coin.inner()));

    // Alice should now have two OwnCoins
    let note: MoneyNote = bob2alice_params.outputs[1].note.decrypt(&th.alice_kp.secret)?;
    let alice_oc = OwnCoin {
        coin: Coin::from(bob2alice_params.outputs[1].coin),
        note: note.clone(),
        secret: th.alice_kp.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.alice_kp.secret.inner(), note.serial])),
        leaf_position: alice_leaf_pos,
    };
    alice_owncoins.push(alice_oc);

    // Bob should have two with the change from the above tx
    let note: MoneyNote = bob2alice_params.outputs[0].note.decrypt(&th.bob_kp.secret)?;
    let bob_oc = OwnCoin {
        coin: Coin::from(bob2alice_params.outputs[0].coin),
        note: note.clone(),
        secret: th.bob_kp.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.bob_kp.secret.inner(), note.serial])),
        leaf_position: bob_leaf_pos,
    };
    bob_owncoins.push(bob_oc);

    assert!(alice_owncoins.len() == 2);
    assert!(bob_owncoins.len() == 2);
    assert!(th.alice_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());
    assert!(th.faucet_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());

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
    // Generating  swap blinds
    let value_send_blind = ValueBlind::random(&mut OsRng);
    let value_recv_blind = ValueBlind::random(&mut OsRng);
    let value_blinds = [value_send_blind, value_recv_blind];
    let token_send_blind = ValueBlind::random(&mut OsRng);
    let token_recv_blind = ValueBlind::random(&mut OsRng);
    let token_blinds = [token_send_blind, token_recv_blind];

    let alice_swap_call_debris = SwapCallBuilder {
        pubkey: th.alice_kp.public,
        value_send: BOB_FIRST_SEND,
        token_id_send: bob_token_id,
        value_recv: ALICE_FIRST_SEND,
        token_id_recv: alice_token_id,
        user_data_blind_send: rcpt_user_data_blind,
        spend_hook_recv: rcpt_spend_hook,
        user_data_recv: rcpt_user_data,
        value_blinds,
        token_blinds,
        coin: alice_owncoins[1].clone(),
        tree: th.alice_merkle_tree.clone(),
        mint_zkbin: th.mint_zkbin.clone(),
        mint_pk: th.mint_pk.clone(),
        burn_zkbin: th.burn_zkbin.clone(),
        burn_pk: th.burn_pk.clone(),
    }
    .build()?;
    let (alice_swap_params, alice_swap_proofs) =
        (alice_swap_call_debris.params, alice_swap_call_debris.proofs);

    assert!(alice_swap_params.inputs.len() == 1);
    assert!(alice_swap_params.outputs.len() == 1);
    alice_owncoins.remove(1);
    assert!(alice_owncoins.len() == 1);

    // Alice sends Bob necessary data and he builds his half.
    info!(target: "money", "[Bob] Building OtcSwap half");
    let bob_swap_call_debris = SwapCallBuilder {
        pubkey: th.bob_kp.public,
        value_send: ALICE_FIRST_SEND,
        token_id_send: alice_token_id,
        value_recv: BOB_FIRST_SEND,
        token_id_recv: bob_token_id,
        user_data_blind_send: rcpt_user_data_blind,
        spend_hook_recv: rcpt_spend_hook,
        user_data_recv: rcpt_user_data,
        value_blinds,
        token_blinds,
        coin: bob_owncoins[0].clone(),
        tree: th.bob_merkle_tree.clone(),
        mint_zkbin: th.mint_zkbin.clone(),
        mint_pk: th.mint_pk.clone(),
        burn_zkbin: th.burn_zkbin.clone(),
        burn_pk: th.burn_pk.clone(),
    }
    .build()?;
    let (bob_swap_params, bob_swap_proofs) =
        (bob_swap_call_debris.params, bob_swap_call_debris.proofs);

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
        calls: vec![ContractCall { contract_id: th.money_contract_id, data }],
        proofs: vec![swap_full_proofs],
        signatures: vec![],
    };
    let sigs = alicebob_swap_tx.create_sigs(&mut OsRng, &[th.bob_kp.secret])?;
    alicebob_swap_tx.signatures = vec![sigs];

    // Alice gets the partially signed transaction and adds her signature
    let sigs = alicebob_swap_tx.create_sigs(&mut OsRng, &[th.alice_kp.secret])?;
    alicebob_swap_tx.signatures[0].insert(0, sigs[0]);

    info!(target: "money", "[Faucet] ==========================");
    info!(target: "money", "[Faucet] Executing AliceBob swap tx");
    info!(target: "money", "[Faucet] ==========================");
    th.faucet_state.read().await.verify_transactions(&[alicebob_swap_tx.clone()], true).await?;
    th.faucet_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[0].coin.inner()));
    th.faucet_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[1].coin.inner()));

    info!(target: "money", "[Alice] ==========================");
    info!(target: "money", "[Alice] Executing AliceBob swap tx");
    info!(target: "money", "[Alice] ==========================");
    th.alice_state.read().await.verify_transactions(&[alicebob_swap_tx.clone()], true).await?;
    th.alice_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[0].coin.inner()));
    let alice_leaf_pos = th.alice_merkle_tree.witness().unwrap();
    th.alice_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[1].coin.inner()));

    info!(target: "money", "[Bob] ==========================");
    info!(target: "money", "[Bob] Executing AliceBob swap tx");
    info!(target: "money", "[Bob] ==========================");
    th.bob_state.read().await.verify_transactions(&[alicebob_swap_tx.clone()], true).await?;
    th.bob_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[0].coin.inner()));
    th.bob_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[1].coin.inner()));
    let bob_leaf_pos = th.bob_merkle_tree.witness().unwrap();

    assert!(th.alice_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());
    assert!(th.faucet_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());

    // Alice should now have two OwnCoins with the same token ID (ALICE)
    let note: MoneyNote = swap_full_params.outputs[0].note.decrypt(&th.alice_kp.secret)?;
    let alice_oc = OwnCoin {
        coin: Coin::from(swap_full_params.outputs[0].coin),
        note: note.clone(),
        secret: th.alice_kp.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.alice_kp.secret.inner(), note.serial])),
        leaf_position: alice_leaf_pos,
    };
    alice_owncoins.push(alice_oc);

    assert!(alice_owncoins.len() == 2);
    assert!(alice_owncoins[0].note.token_id == alice_token_id);
    assert!(alice_owncoins[1].note.token_id == alice_token_id);

    // Same for Bob with BOB tokens
    let note: MoneyNote = swap_full_params.outputs[1].note.decrypt(&th.bob_kp.secret)?;
    let bob_oc = OwnCoin {
        coin: Coin::from(swap_full_params.outputs[1].coin),
        note: note.clone(),
        secret: th.bob_kp.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.bob_kp.secret.inner(), note.serial])),
        leaf_position: bob_leaf_pos,
    };
    bob_owncoins.push(bob_oc);

    assert!(bob_owncoins.len() == 2);
    assert!(bob_owncoins[0].note.token_id == bob_token_id);
    assert!(bob_owncoins[1].note.token_id == bob_token_id);

    // Now Alice will create a new coin for herself to combine the two owncoins.
    info!(target: "money", "[Alice] ======================================================");
    info!(target: "money", "[Alice] Building Money::Transfer params for a payment to Alice");
    info!(target: "money", "[Alice] =======================================================");
    let alice2alice_call_debris = TransferCallBuilder {
        keypair: th.alice_kp,
        recipient: th.alice_kp.public,
        value: ALICE_INITIAL,
        token_id: alice_token_id,
        rcpt_spend_hook,
        rcpt_user_data,
        rcpt_user_data_blind,
        change_spend_hook,
        change_user_data,
        change_user_data_blind,
        coins: alice_owncoins.clone(),
        tree: th.alice_merkle_tree.clone(),
        mint_zkbin: th.mint_zkbin.clone(),
        mint_pk: th.mint_pk.clone(),
        burn_zkbin: th.burn_zkbin.clone(),
        burn_pk: th.burn_pk.clone(),
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
    let calls = vec![ContractCall { contract_id: th.money_contract_id, data }];
    let proofs = vec![alice2alice_proofs];
    let mut alice2alice_tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = alice2alice_tx.create_sigs(&mut OsRng, &alice2alice_secret_keys)?;
    alice2alice_tx.signatures = vec![sigs];

    info!(target: "money", "[Faucet] ================================");
    info!(target: "money", "[Faucet] Executing Alice2Alice payment tx");
    info!(target: "money", "[Faucet] ================================");
    th.faucet_state.read().await.verify_transactions(&[alice2alice_tx.clone()], true).await?;
    th.faucet_merkle_tree.append(&MerkleNode::from(alice2alice_params.outputs[0].coin.inner()));

    info!(target: "money", "[Alice] ================================");
    info!(target: "money", "[Alice] Executing Alice2Alice payment tx");
    info!(target: "money", "[Alice] ================================");
    th.alice_state.read().await.verify_transactions(&[alice2alice_tx.clone()], true).await?;
    th.alice_merkle_tree.append(&MerkleNode::from(alice2alice_params.outputs[0].coin.inner()));
    let alice_leaf_pos = th.alice_merkle_tree.witness().unwrap();

    info!(target: "money", "[Bob] ================================");
    info!(target: "money", "[Bob] Executing Alice2Alice payment tx");
    info!(target: "money", "[Bob] ================================");
    th.bob_state.read().await.verify_transactions(&[alice2alice_tx.clone()], true).await?;
    th.bob_merkle_tree.append(&MerkleNode::from(alice2alice_params.outputs[0].coin.inner()));

    assert!(th.alice_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());
    assert!(th.faucet_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());

    // Alice should now have a single OwnCoin with her initial airdrop
    let note: MoneyNote = alice2alice_params.outputs[0].note.decrypt(&th.alice_kp.secret)?;
    let alice_oc = OwnCoin {
        coin: Coin::from(alice2alice_params.outputs[0].coin),
        note: note.clone(),
        secret: th.alice_kp.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.alice_kp.secret.inner(), note.serial])),
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
    let bob2bob_call_debris = TransferCallBuilder {
        keypair: th.bob_kp,
        recipient: th.bob_kp.public,
        value: BOB_INITIAL,
        token_id: bob_token_id,
        rcpt_spend_hook,
        rcpt_user_data,
        rcpt_user_data_blind,
        change_spend_hook,
        change_user_data,
        change_user_data_blind,
        coins: bob_owncoins.clone(),
        tree: th.bob_merkle_tree.clone(),
        mint_zkbin: th.mint_zkbin.clone(),
        mint_pk: th.mint_pk.clone(),
        burn_zkbin: th.burn_zkbin.clone(),
        burn_pk: th.burn_pk.clone(),
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
    let calls = vec![ContractCall { contract_id: th.money_contract_id, data }];
    let proofs = vec![bob2bob_proofs];
    let mut bob2bob_tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = bob2bob_tx.create_sigs(&mut OsRng, &bob2bob_secret_keys)?;
    bob2bob_tx.signatures = vec![sigs];

    info!(target: "money", "[Faucet] ============================");
    info!(target: "money", "[Faucet] Executing Bob2Bob payment tx");
    info!(target: "money", "[Faucet] ============================");
    th.faucet_state.read().await.verify_transactions(&[bob2bob_tx.clone()], true).await?;
    th.faucet_merkle_tree.append(&MerkleNode::from(bob2bob_params.outputs[0].coin.inner()));

    info!(target: "money", "[Alice] ============================");
    info!(target: "money", "[Alice] Executing Bob2Bob payment tx");
    info!(target: "money", "[Alice] ============================");
    th.alice_state.read().await.verify_transactions(&[bob2bob_tx.clone()], true).await?;
    th.alice_merkle_tree.append(&MerkleNode::from(bob2bob_params.outputs[0].coin.inner()));

    info!(target: "money", "[Bob] ============================");
    info!(target: "money", "[Bob] Executing Bob2Bob payment tx");
    info!(target: "money", "[Bob] ============================");
    th.bob_state.read().await.verify_transactions(&[bob2bob_tx.clone()], true).await?;
    th.bob_merkle_tree.append(&MerkleNode::from(bob2bob_params.outputs[0].coin.inner()));
    let bob_leaf_pos = th.bob_merkle_tree.witness().unwrap();

    assert!(th.alice_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());
    assert!(th.faucet_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());

    // Bob should now have a single OwnCoin with her initial airdrop
    let note: MoneyNote = bob2bob_params.outputs[0].note.decrypt(&th.bob_kp.secret)?;
    let bob_oc = OwnCoin {
        coin: Coin::from(bob2bob_params.outputs[0].coin),
        note: note.clone(),
        secret: th.bob_kp.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.bob_kp.secret.inner(), note.serial])),
        leaf_position: bob_leaf_pos,
    };
    bob_owncoins.push(bob_oc);

    assert!(bob_owncoins.len() == 1);
    assert!(bob_owncoins[0].note.value == BOB_INITIAL);
    assert!(bob_owncoins[0].note.token_id == bob_token_id);

    // Now they decide to swap all of their tokens
    info!(target: "money", "[Alice] Building OtcSwap half");
    // Generating  swap blinds
    let value_send_blind = ValueBlind::random(&mut OsRng);
    let value_recv_blind = ValueBlind::random(&mut OsRng);
    let value_blinds = [value_send_blind, value_recv_blind];
    let token_send_blind = ValueBlind::random(&mut OsRng);
    let token_recv_blind = ValueBlind::random(&mut OsRng);
    let token_blinds = [token_send_blind, token_recv_blind];

    let alice_swap_call_debris = SwapCallBuilder {
        pubkey: th.alice_kp.public,
        value_send: ALICE_INITIAL,
        token_id_send: alice_token_id,
        value_recv: BOB_INITIAL,
        token_id_recv: bob_token_id,
        user_data_blind_send: rcpt_user_data_blind,
        spend_hook_recv: rcpt_spend_hook,
        user_data_recv: rcpt_user_data,
        value_blinds,
        token_blinds,
        coin: alice_owncoins[0].clone(),
        tree: th.alice_merkle_tree.clone(),
        mint_zkbin: th.mint_zkbin.clone(),
        mint_pk: th.mint_pk.clone(),
        burn_zkbin: th.burn_zkbin.clone(),
        burn_pk: th.burn_pk.clone(),
    }
    .build()?;
    let (alice_swap_params, alice_swap_proofs) =
        (alice_swap_call_debris.params, alice_swap_call_debris.proofs);

    assert!(alice_swap_params.inputs.len() == 1);
    assert!(alice_swap_params.outputs.len() == 1);
    alice_owncoins.remove(0);
    assert!(alice_owncoins.is_empty());

    info!(target: "money", "[Bob] Building OtcSwap half");
    let bob_swap_call_debris = SwapCallBuilder {
        pubkey: th.bob_kp.public,
        value_send: BOB_INITIAL,
        token_id_send: bob_token_id,
        value_recv: ALICE_INITIAL,
        token_id_recv: alice_token_id,
        user_data_blind_send: rcpt_user_data_blind,
        spend_hook_recv: rcpt_spend_hook,
        user_data_recv: rcpt_user_data,
        value_blinds,
        token_blinds,
        coin: bob_owncoins[0].clone(),
        tree: th.bob_merkle_tree.clone(),
        mint_zkbin: th.mint_zkbin.clone(),
        mint_pk: th.mint_pk.clone(),
        burn_zkbin: th.burn_zkbin.clone(),
        burn_pk: th.burn_pk.clone(),
    }
    .build()?;
    let (bob_swap_params, bob_swap_proofs) =
        (bob_swap_call_debris.params, bob_swap_call_debris.proofs);

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
        calls: vec![ContractCall { contract_id: th.money_contract_id, data }],
        proofs: vec![swap_full_proofs],
        signatures: vec![],
    };
    let sigs = alicebob_swap_tx.create_sigs(&mut OsRng, &[th.bob_kp.secret])?;
    alicebob_swap_tx.signatures = vec![sigs];

    // Alice gets the partially signed transaction and adds her signature
    let sigs = alicebob_swap_tx.create_sigs(&mut OsRng, &[th.alice_kp.secret])?;
    alicebob_swap_tx.signatures[0].insert(0, sigs[0]);

    info!(target: "money", "[Faucet] ==========================");
    info!(target: "money", "[Faucet] Executing AliceBob swap tx");
    info!(target: "money", "[Faucet] ==========================");
    th.faucet_state.read().await.verify_transactions(&[alicebob_swap_tx.clone()], true).await?;
    th.faucet_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[0].coin.inner()));
    th.faucet_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[1].coin.inner()));

    info!(target: "money", "[Alice] ==========================");
    info!(target: "money", "[Alice] Executing AliceBob swap tx");
    info!(target: "money", "[Alice] ==========================");
    th.alice_state.read().await.verify_transactions(&[alicebob_swap_tx.clone()], true).await?;
    th.alice_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[0].coin.inner()));
    let alice_leaf_pos = th.alice_merkle_tree.witness().unwrap();
    th.alice_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[1].coin.inner()));

    info!(target: "money", "[Bob] ==========================");
    info!(target: "money", "[Bob] Executing AliceBob swap tx");
    info!(target: "money", "[Bob] ==========================");
    th.bob_state.read().await.verify_transactions(&[alicebob_swap_tx.clone()], true).await?;
    th.bob_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[0].coin.inner()));
    th.bob_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[1].coin.inner()));
    let bob_leaf_pos = th.bob_merkle_tree.witness().unwrap();

    assert!(th.alice_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());
    assert!(th.faucet_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());

    // Alice should now have Bob's BOB tokens
    let note: MoneyNote = swap_full_params.outputs[0].note.decrypt(&th.alice_kp.secret)?;
    let alice_oc = OwnCoin {
        coin: Coin::from(swap_full_params.outputs[0].coin),
        note: note.clone(),
        secret: th.alice_kp.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.alice_kp.secret.inner(), note.serial])),
        leaf_position: alice_leaf_pos,
    };
    alice_owncoins.push(alice_oc);

    assert!(alice_owncoins.len() == 1);
    assert!(alice_owncoins[0].note.value == BOB_INITIAL);
    assert!(alice_owncoins[0].note.token_id == bob_token_id);

    // And Bob should have Alice's ALICE tokens
    let note: MoneyNote = swap_full_params.outputs[1].note.decrypt(&th.bob_kp.secret)?;
    let bob_oc = OwnCoin {
        coin: Coin::from(swap_full_params.outputs[1].coin),
        note: note.clone(),
        secret: th.bob_kp.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.bob_kp.secret.inner(), note.serial])),
        leaf_position: bob_leaf_pos,
    };
    bob_owncoins.push(bob_oc);

    assert!(bob_owncoins.len() == 1);
    assert!(bob_owncoins[0].note.value == ALICE_INITIAL);
    assert!(bob_owncoins[0].note.token_id == alice_token_id);

    // Thanks for reading
    Ok(())
}
