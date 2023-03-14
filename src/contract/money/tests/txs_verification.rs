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

//! Test for transaction verification correctness between Alice and Bob.
//!
//! We first mint Alice some tokens, and then she send some to Bob
//! a couple of times, including some double spending transactions.
//!
//! With this test, we want to confirm the transactions execution works
//! between multiple parties, with detection of erroneous transactions.

use darkfi::{tx::Transaction, Result};
use darkfi_sdk::{
    crypto::{
        merkle_prelude::*, pallas, pasta_prelude::*, poseidon_hash, Coin, MerkleNode, Nullifier,
        MONEY_CONTRACT_ID,
    },
    ContractCall,
};
use darkfi_serial::Encodable;
use log::info;
use rand::rngs::OsRng;

use darkfi_money_contract::{
    client::{transfer_v1::TransferCallBuilder, MoneyNote, OwnCoin},
    MoneyFunction::TransferV1 as MoneyTransfer,
    MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};

mod harness;
use harness::{init_logger, MoneyTestHarness};

#[async_std::test]
async fn txs_verification() -> Result<()> {
    init_logger();

    // Some numbers we want to assert
    const ALICE_INITIAL: u64 = 100;

    // Alice = 50 ALICE
    // Bob = 50 ALICE
    const ALICE_FIRST_SEND: u64 = ALICE_INITIAL - 50;

    // Initialize harness
    let mut th = MoneyTestHarness::new().await?;
    let (mint_pk, mint_zkbin) = th.proving_keys.get(&MONEY_CONTRACT_ZKAS_MINT_NS_V1).unwrap();
    let (burn_pk, burn_zkbin) = th.proving_keys.get(&MONEY_CONTRACT_ZKAS_BURN_NS_V1).unwrap();
    let contract_id = *MONEY_CONTRACT_ID;

    // We're just going to be using a zero spend-hook and user-data
    let rcpt_spend_hook = pallas::Base::zero();
    let rcpt_user_data = pallas::Base::zero();
    let rcpt_user_data_blind = pallas::Base::random(&mut OsRng);
    let change_spend_hook = pallas::Base::zero();
    let change_user_data = pallas::Base::zero();
    let change_user_data_blind = pallas::Base::random(&mut OsRng);

    let mut alice_owncoins = vec![];
    let mut bob_owncoins = vec![];

    info!(target: "money", "[Alice] ================================");
    info!(target: "money", "[Alice] Building token mint tx for Alice");
    info!(target: "money", "[Alice] ================================");
    let (alice_mint_tx, alice_params) =
        th.mint_token(th.alice.keypair, ALICE_INITIAL, th.alice.keypair.public)?;

    info!(target: "money", "[Faucet] =============================");
    info!(target: "money", "[Faucet] Executing Alice token mint tx");
    info!(target: "money", "[Faucet] =============================");
    th.faucet.state.read().await.verify_transactions(&[alice_mint_tx.clone()], true).await?;
    th.faucet.merkle_tree.append(&MerkleNode::from(alice_params.output.coin.inner()));

    info!(target: "money", "[Alice] =============================");
    info!(target: "money", "[Alice] Executing Alice token mint tx");
    info!(target: "money", "[Alice] =============================");
    th.alice.state.read().await.verify_transactions(&[alice_mint_tx.clone()], true).await?;
    th.alice.merkle_tree.append(&MerkleNode::from(alice_params.output.coin.inner()));
    // Alice has to witness this coin because it's hers.
    let alice_leaf_pos = th.alice.merkle_tree.witness().unwrap();

    info!(target: "money", "[Bob] =============================");
    info!(target: "money", "[Bob] Executing Alice token mint tx");
    info!(target: "money", "[Bob] =============================");
    th.bob.state.read().await.verify_transactions(&[alice_mint_tx.clone()], true).await?;
    th.bob.merkle_tree.append(&MerkleNode::from(alice_params.output.coin.inner()));

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

    // Now Alice can send a little bit of funds to Bob.
    // We can duplicate this transaction to simulate double spending.
    let DUPLICATES = 1; // Change this number to 2 to double spend
    let mut transactions = vec![];
    let mut txs_params = vec![];
    for i in 0..DUPLICATES {
        info!(target: "money", "[Alice] ======================================================");
        info!(target: "money", "[Alice] Building Money::Transfer params for payment {i} to Bob");
        info!(target: "money", "[Alice] ======================================================");
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

        info!(target: "money", "[Alice] ==============================");
        info!(target: "money", "[Alice] Building payment tx {i} to Bob");
        info!(target: "money", "[Alice] ==============================");
        let mut data = vec![MoneyTransfer as u8];
        alice2bob_params.encode(&mut data)?;
        let calls = vec![ContractCall { contract_id, data }];
        let proofs = vec![alice2bob_proofs];
        let mut alice2bob_tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = alice2bob_tx.create_sigs(&mut OsRng, &alice2bob_secret_keys)?;
        alice2bob_tx.signatures = vec![sigs];

        // Now we simulate nodes verification, as transactions come one by one.
        // Validation should pass, even when we are trying to double spent.
        info!(target: "money", "[Faucet] ==================================");
        info!(target: "money", "[Faucet] Verifying Alice2Bob payment tx {i}");
        info!(target: "money", "[Faucet] ==================================");
        th.faucet.state.read().await.verify_transactions(&[alice2bob_tx.clone()], false).await?;

        info!(target: "money", "[Alice] ==================================");
        info!(target: "money", "[Alice] Verifying Alice2Bob payment tx {i}");
        info!(target: "money", "[Alice] ==================================");
        th.alice.state.read().await.verify_transactions(&[alice2bob_tx.clone()], false).await?;

        info!(target: "money", "[Bob] ==================================");
        info!(target: "money", "[Bob] Verifying Alice2Bob payment tx {i}");
        info!(target: "money", "[Bob] ==================================");
        th.bob.state.read().await.verify_transactions(&[alice2bob_tx.clone()], false).await?;

        transactions.push(alice2bob_tx);
        txs_params.push(alice2bob_params);
    }
    alice_owncoins = vec![];
    assert_eq!(transactions.len(), DUPLICATES);
    assert_eq!(txs_params.len(), DUPLICATES);

    // Now we can try to execute the transactions sequentialy.
    // The first transaction will get applied, while the second one(duplicate) will fail.
    info!(target: "money", "[Faucet] ==============================");
    info!(target: "money", "[Faucet] Executing Alice2Bob payment tx");
    info!(target: "money", "[Faucet] ==============================");
    th.faucet.state.read().await.verify_transactions(&transactions, true).await?;
    th.faucet.merkle_tree.append(&MerkleNode::from(txs_params[0].outputs[0].coin.inner()));
    th.faucet.merkle_tree.append(&MerkleNode::from(txs_params[0].outputs[1].coin.inner()));

    info!(target: "money", "[Alice] ==============================");
    info!(target: "money", "[Alice] Executing Alice2Bob payment tx");
    info!(target: "money", "[Alice] ==============================");
    th.alice.state.read().await.verify_transactions(&transactions, true).await?;
    th.alice.merkle_tree.append(&MerkleNode::from(txs_params[0].outputs[0].coin.inner()));
    let alice_leaf_pos = th.alice.merkle_tree.witness().unwrap();
    th.alice.merkle_tree.append(&MerkleNode::from(txs_params[0].outputs[1].coin.inner()));

    info!(target: "money", "[Bob] ==============================");
    info!(target: "money", "[Bob] Executing Alice2Bob payment tx");
    info!(target: "money", "[Bob] ==============================");
    th.bob.state.read().await.verify_transactions(&transactions, true).await?;
    th.bob.merkle_tree.append(&MerkleNode::from(txs_params[0].outputs[0].coin.inner()));
    th.bob.merkle_tree.append(&MerkleNode::from(txs_params[0].outputs[1].coin.inner()));
    let bob_leaf_pos = th.bob.merkle_tree.witness().unwrap();

    assert!(th.alice.merkle_tree.root(0).unwrap() == th.bob.merkle_tree.root(0).unwrap());
    assert!(th.faucet.merkle_tree.root(0).unwrap() == th.bob.merkle_tree.root(0).unwrap());

    // Alice should now have one OwnCoin with the change from the above transaction.
    let note: MoneyNote = txs_params[0].outputs[0].note.decrypt(&th.alice.keypair.secret)?;
    let alice_oc = OwnCoin {
        coin: Coin::from(txs_params[0].outputs[0].coin),
        note: note.clone(),
        secret: th.alice.keypair.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.alice.keypair.secret.inner(), note.serial])),
        leaf_position: alice_leaf_pos,
    };
    alice_owncoins.push(alice_oc);

    // Bob should now have this new one.
    let note: MoneyNote = txs_params[0].outputs[1].note.decrypt(&th.bob.keypair.secret)?;
    let bob_oc = OwnCoin {
        coin: Coin::from(txs_params[0].outputs[1].coin),
        note: note.clone(),
        secret: th.bob.keypair.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([th.bob.keypair.secret.inner(), note.serial])),
        leaf_position: bob_leaf_pos,
    };
    bob_owncoins.push(bob_oc);

    assert!(alice_owncoins.len() == 1);
    assert!(bob_owncoins.len() == 1);

    // Thanks for reading
    Ok(())
}
