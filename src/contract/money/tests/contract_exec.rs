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

//! In this test module we make sure execution of the contract works as it is
//! intended to. We initialize a state, deploy the contract, create a clear
//! input, and then we try to spend it.
//! Let's see if we manage.
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
use log::{debug, info};
use rand::rngs::OsRng;

use darkfi_money_contract::{
    client::{build_transfer_tx, Coin, EncryptedNote, OwnCoin},
    state::MoneyTransferParams,
    MoneyFunction,
};

#[async_std::test]
async fn money_contract_execution() -> Result<()> {
    // Debug log configuration
    let mut cfg = simplelog::ConfigBuilder::new();
    cfg.add_filter_ignore("sled".to_string());
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        //simplelog::LevelFilter::Info,
        cfg.build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )?;

    // A keypair we can use for the faucet whitelist
    let faucet_kp = Keypair::random(&mut OsRng);

    // A keypair we'll use for Alice
    let alice_kp = Keypair::random(&mut OsRng);

    // The faucet's pubkey is allowed to make clear inputs
    let faucet_pubkeys = vec![faucet_kp.public];

    // The wallets are just noops to get around the ValidatorState API
    let faucet_wallet = WalletDb::new("sqlite::memory:", "foo").await?;
    let alice_wallet = WalletDb::new("sqlite::memory:", "foo").await?;

    // Our main sled database references which live in memory during this test.
    info!("Initializing ValidatorState");
    let faucet_sled_db = sled::Config::new().temporary(true).open()?;
    let alice_sled_db = sled::Config::new().temporary(true).open()?;
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

    // In a hacky way, we just generate the proving keys for the circuits used.
    info!("Looking up zkas circuits from DB");
    let contract_id = ContractId::from(pallas::Base::from(u64::MAX - 420));

    let zkas_mint_ns = String::from("Mint");
    let zkas_burn_ns = String::from("Burn");
    let alice_sled = &alice_state.read().await.blockchain.sled_db;
    let db_handle = alice_state.read().await.blockchain.contracts.lookup(
        alice_sled,
        &contract_id,
        ZKAS_DB_NAME,
    )?;
    let mint_zkbin = db_handle.get(&serialize(&zkas_mint_ns))?.unwrap();
    let burn_zkbin = db_handle.get(&serialize(&zkas_burn_ns))?.unwrap();
    info!("Decoding bincode");
    let mint_zkbin = ZkBinary::decode(&mint_zkbin.clone())?;
    let burn_zkbin = ZkBinary::decode(&burn_zkbin.clone())?;
    let mint_witnesses = empty_witnesses(&mint_zkbin);
    let burn_witnesses = empty_witnesses(&burn_zkbin);
    let mint_circuit = ZkCircuit::new(mint_witnesses, mint_zkbin.clone());
    let burn_circuit = ZkCircuit::new(burn_witnesses, burn_zkbin.clone());

    info!("Creating zk proving keys");
    let k = 13;
    let mut proving_keys = HashMap::<[u8; 32], Vec<(String, ProvingKey)>>::new();
    let mint_pk = ProvingKey::build(k, &mint_circuit);
    let burn_pk = ProvingKey::build(k, &burn_circuit);
    let pks =
        vec![(zkas_mint_ns.clone(), mint_pk.clone()), (zkas_burn_ns.clone(), burn_pk.clone())];
    proving_keys.insert(contract_id.inner().to_repr(), pks);

    // We also have to initialize the Merkle trees used for coins.
    info!("Initializing Merkle trees");
    let mut faucet_merkle_tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(100);
    let mut alice_merkle_tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(100);

    // The faucet will now mint some tokens for Alice.
    let token_id = TokenId::from(pallas::Base::random(&mut OsRng));
    let amount = decode_base10("42.69", 8, true)?;

    info!("Building transfer tx for clear inputs");
    let (params, proofs, secret_keys, _spent_coins) = build_transfer_tx(
        &faucet_kp,
        &alice_kp.public,
        amount,
        token_id,
        &[],
        &faucet_merkle_tree,
        &mint_zkbin,
        &mint_pk,
        &burn_zkbin,
        &burn_pk,
        true,
    )?;

    debug!("PARAMS: {:#?}", params);
    debug!("PROOFS: {:?}", proofs);

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
    info!("Adding coin to faucet's Merkle tree");
    faucet_merkle_tree.append(&MerkleNode::from(params.outputs[0].coin));

    info!("Executing transaction on Alice's blockchain db");
    alice_state.read().await.verify_transactions(&[tx.clone()], true).await?;
    // TODO: FIXME: Actually have a look at the `merkle_add` calls
    alice_merkle_tree.append(&MerkleNode::from(params.outputs[0].coin));
    let leaf_position = alice_merkle_tree.witness().unwrap();

    // If the above succeeded, the state has been written, so Alice should have
    // the minted coin. In practice, Alice's node should get the transaction, scan
    // it, and add it to her wallet. In this test unit, we abstract that away for
    // simplicity reasons. I should make some kind of API for this though.
    info!("Deserializing params");
    let params: MoneyTransferParams = deserialize(&tx.calls[0].data[1..])?;
    let output = &params.outputs[0];
    info!("Decrypting output note");
    let encrypted_note =
        EncryptedNote { ciphertext: output.ciphertext.clone(), ephem_public: output.ephem_public };
    let note = encrypted_note.decrypt(&alice_kp.secret)?;

    // Now since Alice got an output and a note to decrypt, we make the coin
    // metadata so we can spend it.
    let owncoin = OwnCoin {
        coin: Coin::from(output.coin),
        note: note.clone(),
        secret: alice_kp.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([alice_kp.secret.inner(), note.serial])),
        leaf_position,
    };

    // Alice can spend the coin and send another one to herself
    info!("Building transfer tx for Alice from Alice");
    let (params, proofs, secret_keys, _spent_coins) = build_transfer_tx(
        &alice_kp,
        &alice_kp.public,
        amount,
        token_id,
        &[owncoin],
        &alice_merkle_tree,
        &mint_zkbin,
        &mint_pk,
        &burn_zkbin,
        &burn_pk,
        false,
    )?;

    // Build transaction
    let mut data = vec![MoneyFunction::Transfer as u8];
    params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id, data }];
    let proofs = vec![proofs];
    let mut tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = tx.create_sigs(&mut OsRng, &secret_keys)?;
    tx.signatures = vec![sigs];

    info!("Executing transaction on the faucet's blockchain db");
    faucet_state.read().await.verify_transactions(&[tx.clone()], true).await?;
    info!("Adding coin to faucet's Merkle tree");
    faucet_merkle_tree.append(&MerkleNode::from(params.outputs[0].coin));

    info!("Executing transaction on Alice's blockchain db");
    alice_state.read().await.verify_transactions(&[tx.clone()], true).await?;
    // TODO: FIXME: Actually have a look at the `merkle_add` calls
    alice_merkle_tree.append(&MerkleNode::from(params.outputs[0].coin));
    let leaf_position = alice_merkle_tree.witness().unwrap();

    // And again

    info!("Deserializing params");
    let params: MoneyTransferParams = deserialize(&tx.calls[0].data[1..])?;
    let output = &params.outputs[0];
    info!("Decrypting output note");
    let encrypted_note =
        EncryptedNote { ciphertext: output.ciphertext.clone(), ephem_public: output.ephem_public };
    let note = encrypted_note.decrypt(&alice_kp.secret)?;

    // Now since Alice got an output and a note to decrypt, we make the coin
    // metadata so we can spend it.
    let owncoin = OwnCoin {
        coin: Coin::from(output.coin),
        note: note.clone(),
        secret: alice_kp.secret, // <-- What should this be?
        nullifier: Nullifier::from(poseidon_hash([alice_kp.secret.inner(), note.serial])),
        leaf_position,
    };

    // Alice can spend the coin and send another one to herself
    info!("Building transfer tx for Alice from Alice");
    let (params, proofs, secret_keys, _spent_coins) = build_transfer_tx(
        &alice_kp,
        &alice_kp.public,
        amount,
        token_id,
        &[owncoin],
        &alice_merkle_tree,
        &mint_zkbin,
        &mint_pk,
        &burn_zkbin,
        &burn_pk,
        false,
    )?;

    // Build transaction
    let mut data = vec![MoneyFunction::Transfer as u8];
    params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id, data }];
    let proofs = vec![proofs];
    let mut tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = tx.create_sigs(&mut OsRng, &secret_keys)?;
    tx.signatures = vec![sigs];

    info!("Executing transaction on the faucet's blockchain db");
    faucet_state.read().await.verify_transactions(&[tx.clone()], true).await?;
    info!("Adding coin to faucet's Merkle tree");
    faucet_merkle_tree.append(&MerkleNode::from(params.outputs[0].coin));
    info!("Executing transaction on Alice's blockchain db");
    alice_state.read().await.verify_transactions(&[tx], true).await?;
    // TODO: FIXME: Actually have a look at the `merkle_add` calls
    alice_merkle_tree.append(&MerkleNode::from(params.outputs[0].coin));

    Ok(())
}
