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

use std::time::{Duration, Instant};

use darkfi::{tx::Transaction, Result};
use darkfi_sdk::{
    crypto::{pasta_prelude::*, poseidon_hash, MerkleNode, Nullifier, MONEY_CONTRACT_ID},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{serialize, Encodable};
use log::info;
use rand::rngs::OsRng;

use darkfi_money_contract::{
    client::{
        genesis_mint_v1::GenesisMintCallBuilder, transfer_v1::TransferCallBuilder, MoneyNote,
        OwnCoin,
    },
    model::Coin,
    MoneyFunction::{GenesisMintV1 as GenesisMint, TransferV1 as MoneyTransfer},
    MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};

mod harness;
use harness::{init_logger, MoneyTestHarness};

#[async_std::test]
async fn genesis_mint() -> Result<()> {
    init_logger();

    // Some benchmark averages
    let mut genesis_mint_sizes = vec![];
    let mut genesis_mint_broadcasted_sizes = vec![];
    let mut genesis_mint_creation_times = vec![];
    let mut genesis_mint_verify_times = vec![];
    let mut transfer_sizes = vec![];
    let mut transfer_broadcasted_sizes = vec![];
    let mut transfer_creation_times = vec![];
    let mut transfer_verify_times = vec![];

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

    info!(target: "money", "[Alice] ========================");
    info!(target: "money", "[Alice] Building genesis mint tx");
    info!(target: "money", "[Alice] ========================");
    let timer = Instant::now();
    let alice_genesis_mint_call_debris = GenesisMintCallBuilder {
        keypair: th.alice.keypair,
        amount: ALICE_INITIAL,
        spend_hook: rcpt_spend_hook,
        user_data: rcpt_user_data,
        mint_zkbin: mint_zkbin.clone(),
        mint_pk: mint_pk.clone(),
    }
    .build()?;
    let (alice_genesis_mint_params, alice_genesis_mint_proofs) =
        (alice_genesis_mint_call_debris.params, alice_genesis_mint_call_debris.proofs);

    let mut data = vec![GenesisMint as u8];
    alice_genesis_mint_params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id, data }];
    let proofs = vec![alice_genesis_mint_proofs];
    let mut alice_genesis_mint_tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = alice_genesis_mint_tx.create_sigs(&mut OsRng, &[th.alice.keypair.secret])?;
    alice_genesis_mint_tx.signatures = vec![sigs];
    genesis_mint_creation_times.push(timer.elapsed());

    // Calculate transaction sizes
    let encoded: Vec<u8> = serialize(&alice_genesis_mint_tx);
    let size = ::std::mem::size_of_val(&*encoded);
    genesis_mint_sizes.push(size);
    let base58 = bs58::encode(&encoded).into_string();
    let size = ::std::mem::size_of_val(&*base58);
    genesis_mint_broadcasted_sizes.push(size);

    // We are going to use alice genesis mint transaction to
    // test some malicious cases.
    info!(target: "money", "[Malicious] ==================================");
    info!(target: "money", "[Malicious] Checking duplicate genesis mint tx");
    info!(target: "money", "[Malicious] ==================================");
    let erroneous_txs = th
        .alice
        .state
        .read()
        .await
        .verify_transactions(
            &[alice_genesis_mint_tx.clone(), alice_genesis_mint_tx.clone()],
            current_slot,
            false,
        )
        .await?;
    assert_eq!(erroneous_txs.len(), 1);

    info!(target: "money", "[Malicious] ============================================");
    info!(target: "money", "[Malicious] Checking genesis mint tx not on genesis slot");
    info!(target: "money", "[Malicious] ============================================");
    let erroneous_txs = th
        .alice
        .state
        .read()
        .await
        .verify_transactions(&[alice_genesis_mint_tx.clone()], current_slot + 1, false)
        .await?;
    assert_eq!(erroneous_txs.len(), 1);
    info!(target: "money", "[Malicious] ===========================");
    info!(target: "money", "[Malicious] Malicious test cases passed");
    info!(target: "money", "[Malicious] ===========================");

    info!(target: "money", "[Faucet] ===============================");
    info!(target: "money", "[Faucet] Executing Alice genesis mint tx");
    info!(target: "money", "[Faucet] ===============================");
    let timer = Instant::now();
    let erroneous_txs = th
        .faucet
        .state
        .read()
        .await
        .verify_transactions(&[alice_genesis_mint_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.faucet.merkle_tree.append(MerkleNode::from(alice_genesis_mint_params.output.coin.inner()));
    genesis_mint_verify_times.push(timer.elapsed());

    info!(target: "money", "[Alice] ===============================");
    info!(target: "money", "[Alice] Executing Alice genesis mint tx");
    info!(target: "money", "[Alice] ===============================");
    let timer = Instant::now();
    let erroneous_txs = th
        .alice
        .state
        .read()
        .await
        .verify_transactions(&[alice_genesis_mint_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.alice.merkle_tree.append(MerkleNode::from(alice_genesis_mint_params.output.coin.inner()));
    // Alice has to mark this coin because it's hers.
    let alice_leaf_pos = th.alice.merkle_tree.mark().unwrap();
    genesis_mint_verify_times.push(timer.elapsed());

    info!(target: "money", "[Bob] ===============================");
    info!(target: "money", "[Bob] Executing Alice genesis mint tx");
    info!(target: "money", "[Bob] ===============================");
    let timer = Instant::now();
    let erroneous_txs = th
        .bob
        .state
        .read()
        .await
        .verify_transactions(&[alice_genesis_mint_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.bob.merkle_tree.append(MerkleNode::from(alice_genesis_mint_params.output.coin.inner()));
    genesis_mint_verify_times.push(timer.elapsed());

    assert!(th.alice.merkle_tree.root(0).unwrap() == th.bob.merkle_tree.root(0).unwrap());
    assert!(th.faucet.merkle_tree.root(0).unwrap() == th.bob.merkle_tree.root(0).unwrap());

    info!(target: "money", "[Bob] ========================");
    info!(target: "money", "[Bob] Building genesis mint tx");
    info!(target: "money", "[Bob] ========================");
    let timer = Instant::now();
    let bob_genesis_mint_call_debris = GenesisMintCallBuilder {
        keypair: th.bob.keypair,
        amount: BOB_INITIAL,
        spend_hook: rcpt_spend_hook,
        user_data: rcpt_user_data,
        mint_zkbin: mint_zkbin.clone(),
        mint_pk: mint_pk.clone(),
    }
    .build()?;
    let (bob_genesis_mint_params, bob_genesis_mint_proofs) =
        (bob_genesis_mint_call_debris.params, bob_genesis_mint_call_debris.proofs);

    let mut data = vec![GenesisMint as u8];
    bob_genesis_mint_params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id, data }];
    let proofs = vec![bob_genesis_mint_proofs];
    let mut bob_genesis_mint_tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = bob_genesis_mint_tx.create_sigs(&mut OsRng, &[th.bob.keypair.secret])?;
    bob_genesis_mint_tx.signatures = vec![sigs];
    genesis_mint_creation_times.push(timer.elapsed());

    // Calculate transaction sizes
    let encoded: Vec<u8> = serialize(&bob_genesis_mint_tx);
    let size = ::std::mem::size_of_val(&*encoded);
    genesis_mint_sizes.push(size);
    let base58 = bs58::encode(&encoded).into_string();
    let size = ::std::mem::size_of_val(&*base58);
    genesis_mint_broadcasted_sizes.push(size);

    info!(target: "money", "[Faucet] ===============================");
    info!(target: "money", "[Faucet] Executing Bob genesis mint tx");
    info!(target: "money", "[Faucet] ===============================");
    let timer = Instant::now();
    let erroneous_txs = th
        .faucet
        .state
        .read()
        .await
        .verify_transactions(&[bob_genesis_mint_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.faucet.merkle_tree.append(MerkleNode::from(bob_genesis_mint_params.output.coin.inner()));
    genesis_mint_verify_times.push(timer.elapsed());

    info!(target: "money", "[Alice] ===============================");
    info!(target: "money", "[Alice] Executing Bob genesis mint tx");
    info!(target: "money", "[Alice] ===============================");
    let timer = Instant::now();
    let erroneous_txs = th
        .alice
        .state
        .read()
        .await
        .verify_transactions(&[bob_genesis_mint_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.alice.merkle_tree.append(MerkleNode::from(bob_genesis_mint_params.output.coin.inner()));
    genesis_mint_verify_times.push(timer.elapsed());

    info!(target: "money", "[Bob] ===============================");
    info!(target: "money", "[Bob] Executing Bob genesis mint tx");
    info!(target: "money", "[Bob] ===============================");
    let timer = Instant::now();
    let erroneous_txs = th
        .bob
        .state
        .read()
        .await
        .verify_transactions(&[bob_genesis_mint_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());
    th.bob.merkle_tree.append(MerkleNode::from(bob_genesis_mint_params.output.coin.inner()));
    let bob_leaf_pos = th.bob.merkle_tree.mark().unwrap();
    genesis_mint_verify_times.push(timer.elapsed());

    assert!(th.alice.merkle_tree.root(0).unwrap() == th.bob.merkle_tree.root(0).unwrap());
    assert!(th.faucet.merkle_tree.root(0).unwrap() == th.bob.merkle_tree.root(0).unwrap());

    // Alice builds an `OwnCoin` from her genesis mint
    let note: MoneyNote =
        alice_genesis_mint_params.output.note.decrypt(&th.alice.keypair.secret)?;
    let alice_token_id = note.token_id;
    let alice_oc = OwnCoin {
        coin: Coin::from(alice_genesis_mint_params.output.coin),
        note: note.clone(),
        secret: th.alice.keypair.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.alice.keypair.secret.inner(), note.serial])),
        leaf_position: alice_leaf_pos,
    };
    alice_owncoins.push(alice_oc);

    // Bob too
    let note: MoneyNote = bob_genesis_mint_params.output.note.decrypt(&th.bob.keypair.secret)?;
    let bob_token_id = note.token_id;
    let bob_oc = OwnCoin {
        coin: Coin::from(bob_genesis_mint_params.output.coin),
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
        value: ALICE_SEND,
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
    th.faucet.merkle_tree.append(MerkleNode::from(alice2bob_params.outputs[0].coin.inner()));
    th.faucet.merkle_tree.append(MerkleNode::from(alice2bob_params.outputs[1].coin.inner()));
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
    th.alice.merkle_tree.append(MerkleNode::from(alice2bob_params.outputs[0].coin.inner()));
    let alice_leaf_pos = th.alice.merkle_tree.mark().unwrap();
    th.alice.merkle_tree.append(MerkleNode::from(alice2bob_params.outputs[1].coin.inner()));
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
    th.bob.merkle_tree.append(MerkleNode::from(alice2bob_params.outputs[0].coin.inner()));
    th.bob.merkle_tree.append(MerkleNode::from(alice2bob_params.outputs[1].coin.inner()));
    let bob_leaf_pos = th.bob.merkle_tree.mark().unwrap();
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
        value: BOB_SEND,
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
    th.faucet.merkle_tree.append(MerkleNode::from(bob2alice_params.outputs[0].coin.inner()));
    th.faucet.merkle_tree.append(MerkleNode::from(bob2alice_params.outputs[1].coin.inner()));
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
    th.alice.merkle_tree.append(MerkleNode::from(bob2alice_params.outputs[0].coin.inner()));
    th.alice.merkle_tree.append(MerkleNode::from(bob2alice_params.outputs[1].coin.inner()));
    let alice_leaf_pos = th.alice.merkle_tree.mark().unwrap();
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
    th.bob.merkle_tree.append(MerkleNode::from(bob2alice_params.outputs[0].coin.inner()));
    let bob_leaf_pos = th.bob.merkle_tree.mark().unwrap();
    th.bob.merkle_tree.append(MerkleNode::from(bob2alice_params.outputs[1].coin.inner()));
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

    assert!(alice_owncoins[0].note.value == ALICE_INITIAL - ALICE_SEND);
    assert!(alice_owncoins[1].note.value == BOB_SEND);

    assert!(bob_owncoins[0].note.value == ALICE_SEND);
    assert!(bob_owncoins[1].note.value == BOB_INITIAL - BOB_SEND);

    // Statistics
    let genesis_mint_avg = genesis_mint_sizes.iter().sum::<usize>();
    let genesis_mint_avg = genesis_mint_avg / genesis_mint_sizes.len();
    info!("Average Genesis Mint size: {:?} Bytes", genesis_mint_avg);
    let genesis_mint_avg = genesis_mint_broadcasted_sizes.iter().sum::<usize>();
    let genesis_mint_avg = genesis_mint_avg / genesis_mint_broadcasted_sizes.len();
    info!("Average Genesis Mint broadcasted size: {:?} Bytes", genesis_mint_avg);
    let genesis_mint_avg = genesis_mint_creation_times.iter().sum::<Duration>();
    let genesis_mint_avg = genesis_mint_avg / genesis_mint_creation_times.len() as u32;
    info!("Average Genesis Mint creation time: {:?}", genesis_mint_avg);
    let genesis_mint_avg = genesis_mint_verify_times.iter().sum::<Duration>();
    let genesis_mint_avg = genesis_mint_avg / genesis_mint_verify_times.len() as u32;
    info!("Average Genesis Mint verification time: {:?}", genesis_mint_avg);

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

    // Thanks for reading
    Ok(())
}
