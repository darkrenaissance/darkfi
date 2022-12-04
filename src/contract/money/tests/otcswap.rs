/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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
use std::collections::HashMap;

use darkfi::{
    consensus::{
        constants::{TESTNET_GENESIS_HASH_BYTES, TESTNET_GENESIS_TIMESTAMP},
        ValidatorState,
    },
    tx::Transaction,
    util::parse::decode_base10,
    wallet::WalletDb,
    zk::{proof::ProvingKey, vm::ZkCircuit, vm_stack::empty_witnesses},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{
        constants::MERKLE_DEPTH, poseidon_hash, ContractId, Keypair, MerkleNode, Nullifier, TokenId,
    },
    db::ZKAS_DB_NAME,
    incrementalmerkletree::{bridgetree::BridgeTree, Tree},
    pasta::{
        group::ff::{Field, PrimeField},
        pallas,
    },
    tx::ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};
use log::info;
use rand::rngs::OsRng;

use darkfi_money_contract::{
    client::{build_half_swap_tx, build_transfer_tx, Coin, EncryptedNote, OwnCoin},
    state::MoneyTransferParams,
    MoneyFunction, ZKAS_BURN_NS, ZKAS_MINT_NS,
};

#[async_std::test]
async fn money_contract_swap() -> Result<()> {
    // Debug log configuration
    let mut cfg = simplelog::ConfigBuilder::new();
    cfg.add_filter_ignore("sled".to_string());
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        cfg.build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )?;

    // A keypair we can use for the faucet whitelist
    let faucet_kp = Keypair::random(&mut OsRng);

    // A keypair we'll use for Alice
    let alice_kp = Keypair::random(&mut OsRng);

    // A keypair we'll use for Bob
    let bob_kp = Keypair::random(&mut OsRng);

    // The faucet's pubkey is allowed to make clear inputs
    let faucet_pubkeys = vec![faucet_kp.public];

    // The wallets are just noops to get around the ValidatorState API
    let faucet_wallet = WalletDb::new("sqlite::memory:", "foo").await?;
    let alice_wallet = WalletDb::new("sqlite::memory:", "foo").await?;
    let bob_wallet = WalletDb::new("sqlite::memory:", "foo").await?;

    // Our main sled database references which live in memory during this test.
    info!("Initializing ValidatorState");
    let faucet_sled_db = sled::Config::new().temporary(true).open()?;
    let alice_sled_db = sled::Config::new().temporary(true).open()?;
    let bob_sled_db = sled::Config::new().temporary(true).open()?;

    let faucet_state = ValidatorState::new(
        &faucet_sled_db,
        *TESTNET_GENESIS_TIMESTAMP,
        *TESTNET_GENESIS_HASH_BYTES,
        faucet_wallet,
        faucet_pubkeys.clone(),
        false,
    )
    .await?;

    let alice_state = ValidatorState::new(
        &alice_sled_db,
        *TESTNET_GENESIS_TIMESTAMP,
        *TESTNET_GENESIS_HASH_BYTES,
        alice_wallet,
        faucet_pubkeys.clone(),
        false,
    )
    .await?;

    let bob_state = ValidatorState::new(
        &bob_sled_db,
        *TESTNET_GENESIS_TIMESTAMP,
        *TESTNET_GENESIS_HASH_BYTES,
        bob_wallet,
        faucet_pubkeys.clone(),
        false,
    )
    .await?;

    // In a hacky way, we just generate the proving keys for the circuits used.
    info!("Looking up zkas circuits from DB");
    let contract_id = ContractId::from(pallas::Base::from(u64::MAX - 420));

    let alice_sled = &alice_state.read().await.blockchain.sled_db;
    let db_handle = alice_state.read().await.blockchain.contracts.lookup(
        alice_sled,
        &contract_id,
        ZKAS_DB_NAME,
    )?;

    let mint_zkbin = db_handle.get(&serialize(&ZKAS_MINT_NS))?.unwrap();
    let burn_zkbin = db_handle.get(&serialize(&ZKAS_BURN_NS))?.unwrap();
    info!("Decoding bincode");
    let mint_zkbin = ZkBinary::decode(&mint_zkbin.clone())?;
    let burn_zkbin = ZkBinary::decode(&burn_zkbin.clone())?;
    let mint_witnesses = empty_witnesses(&mint_zkbin);
    let burn_witnesses = empty_witnesses(&burn_zkbin);
    let mint_circuit = ZkCircuit::new(mint_witnesses, mint_zkbin.clone());
    let burn_circuit = ZkCircuit::new(burn_witnesses, burn_zkbin.clone());

    info!("Creating zk proving keys");
    let k = 13;
    let mut proving_keys = HashMap::<[u8; 32], Vec<(&str, ProvingKey)>>::new();
    let mint_pk = ProvingKey::build(k, &mint_circuit);
    let burn_pk = ProvingKey::build(k, &burn_circuit);
    let pks = vec![(ZKAS_MINT_NS, mint_pk.clone()), (ZKAS_BURN_NS, burn_pk.clone())];
    proving_keys.insert(contract_id.inner().to_repr(), pks);

    // We also have to initialize the Merkle trees used for coins.
    info!("Initializing Merkle trees");
    let mut faucet_merkle_tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(100);
    let mut alice_merkle_tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(100);
    let mut bob_merkle_tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(100);

    // The faucet will now mint some tokens for Alice and for Bob
    let alice_token_id = TokenId::from(pallas::Base::random(&mut OsRng));
    let bob_token_id = TokenId::from(pallas::Base::random(&mut OsRng));
    let alice_amount = decode_base10("42.69", 8, true)?;
    let bob_amount = decode_base10("69.42", 8, true)?;

    info!("Building transfer tx for Alice's airdrop");
    let (params, proofs, secret_keys, _spent_coins) = build_transfer_tx(
        &faucet_kp,
        &alice_kp.public,
        alice_amount,
        alice_token_id,
        &[],
        &faucet_merkle_tree,
        &mint_zkbin,
        &mint_pk,
        &burn_zkbin,
        &burn_pk,
        true,
    )?;

    // Build transaction
    let mut data = vec![MoneyFunction::Transfer as u8];
    params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id, data }];
    let proofs = vec![proofs];
    let mut tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = tx.create_sigs(&mut OsRng, &secret_keys)?;
    tx.signatures = vec![sigs];

    // Let's first execute this transaction for the faucet to see if it passes.
    // Then Alice gets the tx and also executes it.
    info!("Executing transaction on the faucet's blockchain db");
    faucet_state.read().await.verify_transactions(&[tx.clone()], true).await?;
    faucet_merkle_tree.append(&MerkleNode::from(params.outputs[0].coin));

    info!("Executing transaction on Alice's blockchain db");
    alice_state.read().await.verify_transactions(&[tx.clone()], true).await?;
    // TODO: FIXME: Actually have a look at the `merkle_add` calls
    alice_merkle_tree.append(&MerkleNode::from(params.outputs[0].coin));
    let leaf_position = alice_merkle_tree.witness().unwrap();

    info!("Executing transaction on Bob's blockchain db");
    bob_state.read().await.verify_transactions(&[tx.clone()], true).await?;
    bob_merkle_tree.append(&MerkleNode::from(params.outputs[0].coin));

    let params: MoneyTransferParams = deserialize(&tx.calls[0].data[1..])?;
    let output = &params.outputs[0];
    let encrypted_note =
        EncryptedNote { ciphertext: output.ciphertext.clone(), ephem_public: output.ephem_public };
    let note = encrypted_note.decrypt(&alice_kp.secret)?;

    let mut alice_owncoins = vec![];
    let owncoin = OwnCoin {
        coin: Coin::from(output.coin),
        note: note.clone(),
        secret: alice_kp.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([alice_kp.secret.inner(), note.serial])),
        leaf_position,
    };
    alice_owncoins.push(owncoin);

    info!("Building transfer tx for Bob's airdrop");
    let (params, proofs, secret_keys, _spent_coins) = build_transfer_tx(
        &faucet_kp,
        &bob_kp.public,
        bob_amount,
        bob_token_id,
        &[],
        &faucet_merkle_tree,
        &mint_zkbin,
        &mint_pk,
        &burn_zkbin,
        &burn_pk,
        true,
    )?;

    // Build transaction
    let mut data = vec![MoneyFunction::Transfer as u8];
    params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id, data }];
    let proofs = vec![proofs];
    let mut tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = tx.create_sigs(&mut OsRng, &secret_keys)?;
    tx.signatures = vec![sigs];

    // Let's first execute this transaction for the faucet to see if it passes.
    // Then Alice gets the tx and also executes it.
    info!("Executing transaction on the faucet's blockchain db");
    faucet_state.read().await.verify_transactions(&[tx.clone()], true).await?;
    faucet_merkle_tree.append(&MerkleNode::from(params.outputs[0].coin));

    info!("Executing transaction on Alice's blockchain db");
    alice_state.read().await.verify_transactions(&[tx.clone()], true).await?;
    alice_merkle_tree.append(&MerkleNode::from(params.outputs[0].coin));

    info!("Executing transaction on Bob's blockchain db");
    bob_state.read().await.verify_transactions(&[tx.clone()], true).await?;
    bob_merkle_tree.append(&MerkleNode::from(params.outputs[0].coin));
    let leaf_position = bob_merkle_tree.witness().unwrap();

    let params: MoneyTransferParams = deserialize(&tx.calls[0].data[1..])?;
    let output = &params.outputs[0];
    let encrypted_note =
        EncryptedNote { ciphertext: output.ciphertext.clone(), ephem_public: output.ephem_public };
    let note = encrypted_note.decrypt(&bob_kp.secret)?;

    let mut bob_owncoins = vec![];
    let owncoin = OwnCoin {
        coin: Coin::from(output.coin),
        note: note.clone(),
        secret: bob_kp.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([bob_kp.secret.inner(), note.serial])),
        leaf_position,
    };
    bob_owncoins.push(owncoin);

    // Now Alice and Bob should have their tokens. They can attempt to swap them.
    // Alice will create a transaction half, and send it to Bob, which he can inspect
    // and add his half, sign it, and return to Alice. The Alice can do the inspection
    // and sign with her key, and broadcast the transaction.
    let (
        alice_half_params,
        alice_half_proofs,
        alice_half_keys,
        _alice_half_spent_coins,
        alice_value_blinds,
    ) = build_half_swap_tx(
        &alice_kp.public,
        alice_amount,
        alice_token_id,
        bob_amount,
        bob_token_id,
        &[],
        &alice_owncoins,
        &alice_merkle_tree,
        &mint_zkbin,
        &mint_pk,
        &burn_zkbin,
        &burn_pk,
    )?;

    let (bob_half_params, bob_half_proofs, bob_half_keys, _bob_half_spent_coins, _bob_value_blinds) =
        build_half_swap_tx(
            &bob_kp.public,
            bob_amount,
            bob_token_id,
            alice_amount,
            alice_token_id,
            &alice_value_blinds,
            &bob_owncoins,
            &bob_merkle_tree,
            &mint_zkbin,
            &mint_pk,
            &burn_zkbin,
            &burn_pk,
        )?;

    let bob_full_params = MoneyTransferParams {
        clear_inputs: vec![],
        inputs: vec![alice_half_params.inputs[0].clone(), bob_half_params.inputs[0].clone()],
        outputs: vec![alice_half_params.outputs[0].clone(), bob_half_params.outputs[0].clone()],
    };

    assert!(bob_full_params.inputs.len() == 2);
    assert!(bob_full_params.outputs.len() == 2);

    let mut bob_full_proofs = vec![];
    bob_full_proofs.extend_from_slice(&alice_half_proofs);
    bob_full_proofs.extend_from_slice(&bob_half_proofs);

    let mut data = vec![MoneyFunction::OtcSwap as u8];
    bob_full_params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id, data }];
    let proofs = vec![bob_full_proofs];
    let mut tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = tx.create_sigs(&mut OsRng, &bob_half_keys)?;
    tx.signatures = vec![sigs];

    // This tx finds its way back to Alice.
    // She can try broadcasting the tx without signing, but this should fail to verify.
    info!("[Alice] Verifying half-signed swap transaction (should fail)");
    assert!(alice_state.read().await.verify_transactions(&[tx.clone()], false).await.is_err());

    // So she signs it.
    let sigs = tx.create_sigs(&mut OsRng, &alice_half_keys)?;
    tx.signatures.push(sigs);

    info!("[Alice] Verifying signed swap transaction");
    // Now the transaction is signed by both parties.
    // Let's execute it on Alice's chain state.
    alice_state.read().await.verify_transactions(&[tx.clone()], true).await?;
    // Alice's received coin is in outputs[0]
    alice_merkle_tree.append(&MerkleNode::from(params.outputs[0].coin));
    let leaf_position = alice_merkle_tree.witness().unwrap();
    // This is Bob's received coin
    alice_merkle_tree.append(&MerkleNode::from(params.outputs[1].coin));

    Ok(())
}
