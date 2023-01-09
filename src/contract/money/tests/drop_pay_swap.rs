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
//! We first airdrop them different tokens, and then they send them to each
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
        merkle_prelude::*, pallas, pasta_prelude::*, poseidon_hash, MerkleNode, Nullifier, TokenId,
    },
    ContractCall,
};
use darkfi_serial::Encodable;
use log::info;
use rand::rngs::OsRng;

use darkfi_money_contract::{
    client::{build_half_swap_tx, build_transfer_tx, Coin, EncryptedNote, OwnCoin},
    model::MoneyTransferParams,
    MoneyFunction,
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

    // The faucet will now mint some tokens for Alice and Bob
    let alice_token_id = TokenId::from(pallas::Base::random(&mut OsRng));
    let bob_token_id = TokenId::from(pallas::Base::random(&mut OsRng));

    let mut alice_owncoins = vec![];
    let mut bob_owncoins = vec![];

    info!(target: "money", "[Faucet] ===================================================");
    info!(target: "money", "[Faucet] Building Money::Transfer params for Alice's airdrop");
    info!(target: "money", "[Faucet] ===================================================");
    let (alice_params, alice_proofs, alicedrop_secret_keys, _spent_coins) = build_transfer_tx(
        &th.faucet_kp,
        &th.alice_kp.public,
        ALICE_INITIAL,
        alice_token_id,
        &[],
        &th.faucet_merkle_tree,
        &th.mint_zkbin,
        &th.mint_pk,
        &th.burn_zkbin,
        &th.burn_pk,
        true,
    )?;

    info!(target: "money", "[Faucet] =================================================");
    info!(target: "money", "[Faucet] Building Money::Transfer params for Bob's airdrop");
    info!(target: "money", "[Faucet] =================================================");
    let (bob_params, bob_proofs, bobdrop_secret_keys, _spent_coins) = build_transfer_tx(
        &th.faucet_kp,
        &th.bob_kp.public,
        BOB_INITIAL,
        bob_token_id,
        &[],
        &th.faucet_merkle_tree,
        &th.mint_zkbin,
        &th.mint_pk,
        &th.burn_zkbin,
        &th.burn_pk,
        true,
    )?;

    info!(target: "money", "[Faucet] =====================================");
    info!(target: "money", "[Faucet] Building airdrop tx with Alice params");
    info!(target: "money", "[Faucet] =====================================");
    let mut data = vec![MoneyFunction::Transfer as u8];
    alice_params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id: th.money_contract_id, data }];
    let proofs = vec![alice_proofs];
    let mut alicedrop_tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = alicedrop_tx.create_sigs(&mut OsRng, &alicedrop_secret_keys)?;
    alicedrop_tx.signatures = vec![sigs];

    info!(target: "money", "[Faucet] ===================================");
    info!(target: "money", "[Faucet] Building airdrop tx with Bob params");
    info!(target: "money", "[Faucet] ===================================");
    let mut data = vec![MoneyFunction::Transfer as u8];
    bob_params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id: th.money_contract_id, data }];
    let proofs = vec![bob_proofs];
    let mut bobdrop_tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = bobdrop_tx.create_sigs(&mut OsRng, &bobdrop_secret_keys)?;
    bobdrop_tx.signatures = vec![sigs];

    info!(target: "money", "[Faucet] ==========================");
    info!(target: "money", "[Faucet] Executing Alice airdrop tx");
    info!(target: "money", "[Faucet] ==========================");
    th.faucet_state.read().await.verify_transactions(&[alicedrop_tx.clone()], true).await?;
    th.faucet_merkle_tree.append(&MerkleNode::from(alice_params.outputs[0].coin));

    info!(target: "money", "[Faucet] ========================");
    info!(target: "money", "[Faucet] Executing Bob airdrop tx");
    info!(target: "money", "[Faucet] ========================");
    th.faucet_state.read().await.verify_transactions(&[bobdrop_tx.clone()], true).await?;
    th.faucet_merkle_tree.append(&MerkleNode::from(bob_params.outputs[0].coin));

    info!(target: "money", "[Alice] ==========================");
    info!(target: "money", "[Alice] Executing Alice airdrop tx");
    info!(target: "money", "[Alice] ==========================");
    th.alice_state.read().await.verify_transactions(&[alicedrop_tx.clone()], true).await?;
    th.alice_merkle_tree.append(&MerkleNode::from(alice_params.outputs[0].coin));
    // Alice has to witness this coin because it's hers.
    let alice_leaf_pos = th.alice_merkle_tree.witness().unwrap();

    info!(target: "money", "[Alice] ========================");
    info!(target: "money", "[Alice] Executing Bob airdrop tx");
    info!(target: "money", "[Alice] ========================");
    th.alice_state.read().await.verify_transactions(&[bobdrop_tx.clone()], true).await?;
    th.alice_merkle_tree.append(&MerkleNode::from(bob_params.outputs[0].coin));

    info!(target: "money", "[Bob] ==========================");
    info!(target: "money", "[Bob] Executing Alice airdrop tx");
    info!(target: "money", "[Bob] ==========================");
    th.bob_state.read().await.verify_transactions(&[alicedrop_tx.clone()], true).await?;
    th.bob_merkle_tree.append(&MerkleNode::from(alice_params.outputs[0].coin));

    info!(target: "money", "[Bob] ========================");
    info!(target: "money", "[Bob] Executing Bob airdrop tx");
    info!(target: "money", "[Bob] ========================");
    th.bob_state.read().await.verify_transactions(&[bobdrop_tx.clone()], true).await?;
    th.bob_merkle_tree.append(&MerkleNode::from(bob_params.outputs[0].coin));
    let bob_leaf_pos = th.bob_merkle_tree.witness().unwrap();

    assert!(th.alice_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());
    assert!(th.faucet_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());

    // Alice builds an `OwnCoin` from her airdrop
    let ciphertext = alice_params.outputs[0].ciphertext.clone();
    let ephem_public = alice_params.outputs[0].ephem_public;
    let e_note = EncryptedNote { ciphertext, ephem_public };
    let note = e_note.decrypt(&th.alice_kp.secret)?;
    let alice_oc = OwnCoin {
        coin: Coin::from(alice_params.outputs[0].coin),
        note: note.clone(),
        secret: th.alice_kp.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.alice_kp.secret.inner(), note.serial])),
        leaf_position: alice_leaf_pos,
    };
    alice_owncoins.push(alice_oc);

    // Bob too
    let ciphertext = bob_params.outputs[0].ciphertext.clone();
    let ephem_public = bob_params.outputs[0].ephem_public;
    let e_note = EncryptedNote { ciphertext, ephem_public };
    let note = e_note.decrypt(&th.bob_kp.secret)?;
    let bob_oc = OwnCoin {
        coin: Coin::from(bob_params.outputs[0].coin),
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
    let (alice2bob_params, alice2bob_proofs, alice2bob_secret_keys, alice2bob_spent_coins) =
        build_transfer_tx(
            &th.alice_kp,
            &th.bob_kp.public,
            ALICE_FIRST_SEND,
            alice_token_id,
            &alice_owncoins,
            &th.alice_merkle_tree,
            &th.mint_zkbin,
            &th.mint_pk,
            &th.burn_zkbin,
            &th.burn_pk,
            false,
        )?;

    assert!(alice2bob_params.inputs.len() == 1);
    assert!(alice2bob_params.outputs.len() == 2);
    assert!(alice2bob_spent_coins.len() == 1);
    alice_owncoins.retain(|x| x != &alice2bob_spent_coins[0]);
    assert!(alice_owncoins.is_empty());

    info!(target: "money", "[Alice] ==========================");
    info!(target: "money", "[Alice] Building payment tx to Bob");
    info!(target: "money", "[Alice] ==========================");
    let mut data = vec![MoneyFunction::Transfer as u8];
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
    th.faucet_merkle_tree.append(&MerkleNode::from(alice2bob_params.outputs[0].coin));
    th.faucet_merkle_tree.append(&MerkleNode::from(alice2bob_params.outputs[1].coin));

    info!(target: "money", "[Alice] ==============================");
    info!(target: "money", "[Alice] Executing Alice2Bob payment tx");
    info!(target: "money", "[Alice] ==============================");
    th.alice_state.read().await.verify_transactions(&[alice2bob_tx.clone()], true).await?;
    th.alice_merkle_tree.append(&MerkleNode::from(alice2bob_params.outputs[0].coin));
    let alice_leaf_pos = th.alice_merkle_tree.witness().unwrap();
    th.alice_merkle_tree.append(&MerkleNode::from(alice2bob_params.outputs[1].coin));

    info!(target: "money", "[Bob] ==============================");
    info!(target: "money", "[Bob] Executing Alice2Bob payment tx");
    info!(target: "money", "[Bob] ==============================");
    th.bob_state.read().await.verify_transactions(&[alice2bob_tx.clone()], true).await?;
    th.bob_merkle_tree.append(&MerkleNode::from(alice2bob_params.outputs[0].coin));
    th.bob_merkle_tree.append(&MerkleNode::from(alice2bob_params.outputs[1].coin));
    let bob_leaf_pos = th.bob_merkle_tree.witness().unwrap();

    assert!(th.alice_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());
    assert!(th.faucet_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());

    // Alice should now have one OwnCoin with the change from the above transaction.
    let ciphertext = alice2bob_params.outputs[0].ciphertext.clone();
    let ephem_public = alice2bob_params.outputs[0].ephem_public;
    let e_note = EncryptedNote { ciphertext, ephem_public };
    let note = e_note.decrypt(&th.alice_kp.secret)?;
    let alice_oc = OwnCoin {
        coin: Coin::from(alice2bob_params.outputs[0].coin),
        note: note.clone(),
        secret: th.alice_kp.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.alice_kp.secret.inner(), note.serial])),
        leaf_position: alice_leaf_pos,
    };
    alice_owncoins.push(alice_oc);

    // Bob should have his old one, and this new one.
    let ciphertext = alice2bob_params.outputs[1].ciphertext.clone();
    let ephem_public = alice2bob_params.outputs[1].ephem_public;
    let e_note = EncryptedNote { ciphertext, ephem_public };
    let note = e_note.decrypt(&th.bob_kp.secret)?;
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
    let (bob2alice_params, bob2alice_proofs, bob2alice_secret_keys, bob2alice_spent_coins) =
        build_transfer_tx(
            &th.bob_kp,
            &th.alice_kp.public,
            BOB_FIRST_SEND,
            bob_token_id,
            &bob_owncoins_tmp,
            &th.bob_merkle_tree,
            &th.mint_zkbin,
            &th.mint_pk,
            &th.burn_zkbin,
            &th.burn_pk,
            false,
        )?;

    assert!(bob2alice_params.inputs.len() == 1);
    assert!(bob2alice_params.outputs.len() == 2);
    assert!(bob2alice_spent_coins.len() == 1);
    bob_owncoins.retain(|x| x != &bob2alice_spent_coins[0]);
    assert!(bob_owncoins.len() == 1);

    info!(target: "money", "[Bob] ============================");
    info!(target: "money", "[Bob] Building payment tx to Alice");
    info!(target: "money", "[Bob] ============================");
    let mut data = vec![MoneyFunction::Transfer as u8];
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
    th.faucet_merkle_tree.append(&MerkleNode::from(bob2alice_params.outputs[0].coin));
    th.faucet_merkle_tree.append(&MerkleNode::from(bob2alice_params.outputs[1].coin));

    info!(target: "money", "[Alice] ==============================");
    info!(target: "money", "[Alice] Executing Bob2Alice payment tx");
    info!(target: "money", "[Alice] ==============================");
    th.alice_state.read().await.verify_transactions(&[bob2alice_tx.clone()], true).await?;
    th.alice_merkle_tree.append(&MerkleNode::from(bob2alice_params.outputs[0].coin));
    th.alice_merkle_tree.append(&MerkleNode::from(bob2alice_params.outputs[1].coin));
    let alice_leaf_pos = th.alice_merkle_tree.witness().unwrap();

    info!(target: "money", "[Bob] ==================+===========");
    info!(target: "money", "[Bob] Executing Bob2Alice payment tx");
    info!(target: "money", "[Bob] ==================+===========");
    th.bob_state.read().await.verify_transactions(&[bob2alice_tx.clone()], true).await?;
    th.bob_merkle_tree.append(&MerkleNode::from(bob2alice_params.outputs[0].coin));
    let bob_leaf_pos = th.bob_merkle_tree.witness().unwrap();
    th.bob_merkle_tree.append(&MerkleNode::from(bob2alice_params.outputs[1].coin));

    // Alice should now have two OwnCoins
    let ciphertext = bob2alice_params.outputs[1].ciphertext.clone();
    let ephem_public = bob2alice_params.outputs[1].ephem_public;
    let e_note = EncryptedNote { ciphertext, ephem_public };
    let note = e_note.decrypt(&th.alice_kp.secret)?;
    let alice_oc = OwnCoin {
        coin: Coin::from(bob2alice_params.outputs[1].coin),
        note: note.clone(),
        secret: th.alice_kp.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.alice_kp.secret.inner(), note.serial])),
        leaf_position: alice_leaf_pos,
    };
    alice_owncoins.push(alice_oc);

    // Bob should have two with the change from the above tx
    let ciphertext = bob2alice_params.outputs[0].ciphertext.clone();
    let ephem_public = bob2alice_params.outputs[0].ephem_public;
    let e_note = EncryptedNote { ciphertext, ephem_public };
    let note = e_note.decrypt(&th.bob_kp.secret)?;
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
    let (
        alice_swap_params,
        alice_swap_proofs,
        alice_swap_secret_keys,
        alice_swap_spent_coins,
        alice_value_blinds,
        alice_token_blinds,
    ) = build_half_swap_tx(
        &th.alice_kp.public,
        BOB_FIRST_SEND,
        bob_token_id,
        ALICE_FIRST_SEND,
        alice_token_id,
        &[],
        &[],
        &[alice_owncoins[1].clone()],
        &th.alice_merkle_tree,
        &th.mint_zkbin,
        &th.mint_pk,
        &th.burn_zkbin,
        &th.burn_pk,
    )?;

    assert!(alice_swap_params.inputs.len() == 1);
    assert!(alice_swap_params.outputs.len() == 1);
    assert!(alice_swap_spent_coins.len() == 1);
    alice_owncoins.retain(|x| x != &alice_swap_spent_coins[0]);
    assert!(alice_owncoins.len() == 1);

    // Alice sends Bob necessary data and he builds his half.
    info!(target: "money", "[Bob] Building OtcSwap half");
    let (
        bob_swap_params,
        bob_swap_proofs,
        bob_swap_secret_keys,
        bob_swap_spent_coins,
        _bob_value_blinds,
        _bob_token_blinds,
    ) = build_half_swap_tx(
        &th.bob_kp.public,
        ALICE_FIRST_SEND,
        alice_token_id,
        BOB_FIRST_SEND,
        bob_token_id,
        &alice_value_blinds,
        &alice_token_blinds,
        &[bob_owncoins[0].clone()],
        &th.bob_merkle_tree,
        &th.mint_zkbin,
        &th.mint_pk,
        &th.burn_zkbin,
        &th.burn_pk,
    )?;

    assert!(bob_swap_params.inputs.len() == 1);
    assert!(bob_swap_params.outputs.len() == 1);
    assert!(bob_swap_spent_coins.len() == 1);
    bob_owncoins.retain(|x| x != &bob_swap_spent_coins[0]);
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
    let mut data = vec![MoneyFunction::OtcSwap as u8];
    swap_full_params.encode(&mut data)?;
    let mut alicebob_swap_tx = Transaction {
        calls: vec![ContractCall { contract_id: th.money_contract_id, data }],
        proofs: vec![swap_full_proofs],
        signatures: vec![],
    };
    let sigs = alicebob_swap_tx.create_sigs(&mut OsRng, &bob_swap_secret_keys)?;
    alicebob_swap_tx.signatures = vec![sigs];

    // Alice gets the partially signed transaction and adds her signature
    let sigs = alicebob_swap_tx.create_sigs(&mut OsRng, &alice_swap_secret_keys)?;
    alicebob_swap_tx.signatures[0].insert(0, sigs[0]);

    info!(target: "money", "[Faucet] ==========================");
    info!(target: "money", "[Faucet] Executing AliceBob swap tx");
    info!(target: "money", "[Faucet] ==========================");
    th.faucet_state.read().await.verify_transactions(&[alicebob_swap_tx.clone()], true).await?;
    th.faucet_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[0].coin));
    th.faucet_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[1].coin));

    info!(target: "money", "[Alice] ==========================");
    info!(target: "money", "[Alice] Executing AliceBob swap tx");
    info!(target: "money", "[Alice] ==========================");
    th.alice_state.read().await.verify_transactions(&[alicebob_swap_tx.clone()], true).await?;
    th.alice_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[0].coin));
    let alice_leaf_pos = th.alice_merkle_tree.witness().unwrap();
    th.alice_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[1].coin));

    info!(target: "money", "[Bob] ==========================");
    info!(target: "money", "[Bob] Executing AliceBob swap tx");
    info!(target: "money", "[Bob] ==========================");
    th.bob_state.read().await.verify_transactions(&[alicebob_swap_tx.clone()], true).await?;
    th.bob_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[0].coin));
    th.bob_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[1].coin));
    let bob_leaf_pos = th.bob_merkle_tree.witness().unwrap();

    assert!(th.alice_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());
    assert!(th.faucet_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());

    // Alice should now have two OwnCoins with the same token ID (ALICE)
    let ciphertext = swap_full_params.outputs[0].ciphertext.clone();
    let ephem_public = swap_full_params.outputs[0].ephem_public;
    let e_note = EncryptedNote { ciphertext, ephem_public };
    let note = e_note.decrypt(&th.alice_kp.secret)?;
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
    let ciphertext = swap_full_params.outputs[1].ciphertext.clone();
    let ephem_public = swap_full_params.outputs[1].ephem_public;
    let e_note = EncryptedNote { ciphertext, ephem_public };
    let note = e_note.decrypt(&th.bob_kp.secret)?;
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
    let (alice2alice_params, alice2alice_proofs, alice2alice_secret_keys, alice2alice_spent_coins) =
        build_transfer_tx(
            &th.alice_kp,
            &th.alice_kp.public,
            ALICE_INITIAL,
            alice_token_id,
            &alice_owncoins,
            &th.alice_merkle_tree,
            &th.mint_zkbin,
            &th.mint_pk,
            &th.burn_zkbin,
            &th.burn_pk,
            false,
        )?;

    for coin in alice2alice_spent_coins {
        alice_owncoins.retain(|x| x != &coin);
    }
    assert!(alice_owncoins.is_empty());
    assert!(alice2alice_params.inputs.len() == 2);
    assert!(alice2alice_params.outputs.len() == 1);

    info!(target: "money", "[Alice] ============================");
    info!(target: "money", "[Alice] Building payment tx to Alice");
    info!(target: "money", "[Alice] ============================");
    let mut data = vec![MoneyFunction::Transfer as u8];
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
    th.faucet_merkle_tree.append(&MerkleNode::from(alice2alice_params.outputs[0].coin));

    info!(target: "money", "[Alice] ================================");
    info!(target: "money", "[Alice] Executing Alice2Alice payment tx");
    info!(target: "money", "[Alice] ================================");
    th.alice_state.read().await.verify_transactions(&[alice2alice_tx.clone()], true).await?;
    th.alice_merkle_tree.append(&MerkleNode::from(alice2alice_params.outputs[0].coin));
    let alice_leaf_pos = th.alice_merkle_tree.witness().unwrap();

    info!(target: "money", "[Bob] ================================");
    info!(target: "money", "[Bob] Executing Alice2Alice payment tx");
    info!(target: "money", "[Bob] ================================");
    th.bob_state.read().await.verify_transactions(&[alice2alice_tx.clone()], true).await?;
    th.bob_merkle_tree.append(&MerkleNode::from(alice2alice_params.outputs[0].coin));

    assert!(th.alice_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());
    assert!(th.faucet_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());

    // Alice should now have a single OwnCoin with her initial airdrop
    let ciphertext = alice2alice_params.outputs[0].ciphertext.clone();
    let ephem_public = alice2alice_params.outputs[0].ephem_public;
    let e_note = EncryptedNote { ciphertext, ephem_public };
    let note = e_note.decrypt(&th.alice_kp.secret)?;
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
    let (bob2bob_params, bob2bob_proofs, bob2bob_secret_keys, bob2bob_spent_coins) =
        build_transfer_tx(
            &th.bob_kp,
            &th.bob_kp.public,
            BOB_INITIAL,
            bob_token_id,
            &bob_owncoins,
            &th.bob_merkle_tree,
            &th.mint_zkbin,
            &th.mint_pk,
            &th.burn_zkbin,
            &th.burn_pk,
            false,
        )?;

    for coin in bob2bob_spent_coins {
        bob_owncoins.retain(|x| x != &coin);
    }
    assert!(bob_owncoins.is_empty());
    assert!(bob2bob_params.inputs.len() == 2);
    assert!(bob2bob_params.outputs.len() == 1);

    info!(target: "money", "[Bob] ==========================");
    info!(target: "money", "[Bob] Building payment tx to Bob");
    info!(target: "money", "[Bob] ==========================");
    let mut data = vec![MoneyFunction::Transfer as u8];
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
    th.faucet_merkle_tree.append(&MerkleNode::from(bob2bob_params.outputs[0].coin));

    info!(target: "money", "[Alice] ============================");
    info!(target: "money", "[Alice] Executing Bob2Bob payment tx");
    info!(target: "money", "[Alice] ============================");
    th.alice_state.read().await.verify_transactions(&[bob2bob_tx.clone()], true).await?;
    th.alice_merkle_tree.append(&MerkleNode::from(bob2bob_params.outputs[0].coin));

    info!(target: "money", "[Bob] ============================");
    info!(target: "money", "[Bob] Executing Bob2Bob payment tx");
    info!(target: "money", "[Bob] ============================");
    th.bob_state.read().await.verify_transactions(&[bob2bob_tx.clone()], true).await?;
    th.bob_merkle_tree.append(&MerkleNode::from(bob2bob_params.outputs[0].coin));
    let bob_leaf_pos = th.bob_merkle_tree.witness().unwrap();

    assert!(th.alice_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());
    assert!(th.faucet_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());

    // Bob should now have a single OwnCoin with her initial airdrop
    let ciphertext = bob2bob_params.outputs[0].ciphertext.clone();
    let ephem_public = bob2bob_params.outputs[0].ephem_public;
    let e_note = EncryptedNote { ciphertext, ephem_public };
    let note = e_note.decrypt(&th.bob_kp.secret)?;
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
    let (
        alice_swap_params,
        alice_swap_proofs,
        alice_swap_secret_keys,
        alice_swap_spent_coins,
        alice_value_blinds,
        alice_token_blinds,
    ) = build_half_swap_tx(
        &th.alice_kp.public,
        ALICE_INITIAL,
        alice_token_id,
        BOB_INITIAL,
        bob_token_id,
        &[],
        &[],
        &alice_owncoins,
        &th.alice_merkle_tree,
        &th.mint_zkbin,
        &th.mint_pk,
        &th.burn_zkbin,
        &th.burn_pk,
    )?;

    assert!(alice_swap_params.inputs.len() == 1);
    assert!(alice_swap_params.outputs.len() == 1);
    assert!(alice_swap_spent_coins.len() == 1);
    alice_owncoins.retain(|x| x != &alice_swap_spent_coins[0]);
    assert!(alice_owncoins.is_empty());

    info!(target: "money", "[Bob] Building OtcSwap half");
    let (
        bob_swap_params,
        bob_swap_proofs,
        bob_swap_secret_keys,
        bob_swap_spent_coins,
        _bob_value_blinds,
        _bob_token_blinds,
    ) = build_half_swap_tx(
        &th.bob_kp.public,
        BOB_INITIAL,
        bob_token_id,
        ALICE_INITIAL,
        alice_token_id,
        &alice_value_blinds,
        &alice_token_blinds,
        &bob_owncoins,
        &th.bob_merkle_tree,
        &th.mint_zkbin,
        &th.mint_pk,
        &th.burn_zkbin,
        &th.burn_pk,
    )?;

    assert!(bob_swap_params.inputs.len() == 1);
    assert!(bob_swap_params.outputs.len() == 1);
    assert!(bob_swap_spent_coins.len() == 1);
    bob_owncoins.retain(|x| x != &bob_swap_spent_coins[0]);
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
    let mut data = vec![MoneyFunction::OtcSwap as u8];
    swap_full_params.encode(&mut data)?;
    let mut alicebob_swap_tx = Transaction {
        calls: vec![ContractCall { contract_id: th.money_contract_id, data }],
        proofs: vec![swap_full_proofs],
        signatures: vec![],
    };
    let sigs = alicebob_swap_tx.create_sigs(&mut OsRng, &bob_swap_secret_keys)?;
    alicebob_swap_tx.signatures = vec![sigs];

    // Alice gets the partially signed transaction and adds her signature
    let sigs = alicebob_swap_tx.create_sigs(&mut OsRng, &alice_swap_secret_keys)?;
    alicebob_swap_tx.signatures[0].insert(0, sigs[0]);

    info!(target: "money", "[Faucet] ==========================");
    info!(target: "money", "[Faucet] Executing AliceBob swap tx");
    info!(target: "money", "[Faucet] ==========================");
    th.faucet_state.read().await.verify_transactions(&[alicebob_swap_tx.clone()], true).await?;
    th.faucet_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[0].coin));
    th.faucet_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[1].coin));

    info!(target: "money", "[Alice] ==========================");
    info!(target: "money", "[Alice] Executing AliceBob swap tx");
    info!(target: "money", "[Alice] ==========================");
    th.alice_state.read().await.verify_transactions(&[alicebob_swap_tx.clone()], true).await?;
    th.alice_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[0].coin));
    let alice_leaf_pos = th.alice_merkle_tree.witness().unwrap();
    th.alice_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[1].coin));

    info!(target: "money", "[Bob] ==========================");
    info!(target: "money", "[Bob] Executing AliceBob swap tx");
    info!(target: "money", "[Bob] ==========================");
    th.bob_state.read().await.verify_transactions(&[alicebob_swap_tx.clone()], true).await?;
    th.bob_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[0].coin));
    th.bob_merkle_tree.append(&MerkleNode::from(swap_full_params.outputs[1].coin));
    let bob_leaf_pos = th.bob_merkle_tree.witness().unwrap();

    assert!(th.alice_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());
    assert!(th.faucet_merkle_tree.root(0).unwrap() == th.bob_merkle_tree.root(0).unwrap());

    // Alice should now have Bob's BOB tokens
    let ciphertext = swap_full_params.outputs[0].ciphertext.clone();
    let ephem_public = swap_full_params.outputs[0].ephem_public;
    let e_note = EncryptedNote { ciphertext, ephem_public };
    let note = e_note.decrypt(&th.alice_kp.secret)?;
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
    let ciphertext = swap_full_params.outputs[1].ciphertext.clone();
    let ephem_public = swap_full_params.outputs[1].ephem_public;
    let e_note = EncryptedNote { ciphertext, ephem_public };
    let note = e_note.decrypt(&th.bob_kp.secret)?;
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
