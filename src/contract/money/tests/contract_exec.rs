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
use std::{collections::HashMap, io::Cursor};

use darkfi::{
    blockchain::Blockchain,
    consensus::constants::{TESTNET_GENESIS_HASH_BYTES, TESTNET_GENESIS_TIMESTAMP},
    crypto::proof::{ProvingKey, VerifyingKey},
    runtime::vm_runtime::Runtime,
    tx::Transaction,
    util::parse::decode_base10,
    zk::{vm::ZkCircuit, vm_stack::empty_witnesses},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{
        constants::MERKLE_DEPTH, poseidon_hash, ContractId, Keypair, MerkleNode, Nullifier,
        PublicKey, TokenId,
    },
    incrementalmerkletree::{bridgetree::BridgeTree, Tree},
    pasta::{
        group::ff::{Field, PrimeField},
        pallas,
    },
    tx::ContractCall,
};
use darkfi_serial::{deserialize, serialize, Decodable, Encodable, WriteExt};
use log::info;
use rand::rngs::OsRng;

use darkfi_money_contract::{
    client::{build_transfer_tx, Coin, EncryptedNote, OwnCoin},
    state::MoneyTransferParams,
    instruction::MoneyFunction,
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

    // Our main sled database references which live in memory during this test.
    info!("Initializing sled DBs");
    let faucet_sled_db = sled::Config::new().temporary(true).open()?;
    let alice_sled_db = sled::Config::new().temporary(true).open()?;
    let faucet_blockchain =
        Blockchain::new(&faucet_sled_db, *TESTNET_GENESIS_TIMESTAMP, *TESTNET_GENESIS_HASH_BYTES)?;
    let alice_blockchain =
        Blockchain::new(&alice_sled_db, *TESTNET_GENESIS_TIMESTAMP, *TESTNET_GENESIS_HASH_BYTES)?;

    // A keypair we can use for the faucet whitelist
    let faucet_kp = Keypair::random(&mut OsRng);

    // A keypair we'll use for Alice
    let alice_kp = Keypair::random(&mut OsRng);

    // We deploy the contract natively and initialize its state.
    info!("Deploying WASM contract");
    let wasm_bincode = include_bytes!("../money_contract.wasm");
    let contract_id = ContractId::from(pallas::Base::from(u64::MAX - 420));
    let mut faucet_runtime =
        Runtime::new(&wasm_bincode[..], faucet_blockchain.clone(), contract_id)?;
    let mut alice_runtime = Runtime::new(&wasm_bincode[..], alice_blockchain.clone(), contract_id)?;

    let faucet_pubkeys = vec![faucet_kp.public];
    // Serialize the payload for the init/deploy function of the contract and run the deploy.
    let payload = serialize(&faucet_pubkeys);
    faucet_runtime.deploy(&payload)?;
    alice_runtime.deploy(&payload)?;

    // At this point we've deployed the contract and we can begin executing it.
    // When the contract is deployed, we should be able to access everything from
    // the sled databases. We do it here just to confirm correct behaviour.
    info!("Looking up zkas circuits from DB");
    let zkas_tree = String::from("zkas");
    let zkas_mint_ns = String::from("Mint");
    let zkas_burn_ns = String::from("Burn");
    let db_handle =
        alice_blockchain.contracts.lookup(&alice_blockchain.sled_db, &contract_id, &zkas_tree)?;
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
    // 'k' used for defining the number of rows in the zkvm for a certain circuit, currently hardcoded to 13
    // It's going to be dynamic in the future when the zkvm learns how to self-optimize
    let k = 13;
    let mut proving_keys = HashMap::<[u8; 32], Vec<(String, ProvingKey)>>::new();
    let mint_pk = ProvingKey::build(k, &mint_circuit);
    let burn_pk = ProvingKey::build(k, &burn_circuit);
    let pks =
        vec![(zkas_mint_ns.clone(), mint_pk.clone()), (zkas_burn_ns.clone(), burn_pk.clone())];
    proving_keys.insert(contract_id.inner().to_repr(), pks);

    info!("Creating zk verifying keys");
    let mut verifying_keys = HashMap::<[u8; 32], Vec<(String, VerifyingKey)>>::new();
    let mint_vk = VerifyingKey::build(k, &mint_circuit);
    let burn_vk = VerifyingKey::build(k, &burn_circuit);
    let vks =
        vec![(zkas_mint_ns.clone(), mint_vk.clone()), (zkas_burn_ns.clone(), burn_vk.clone())];
    verifying_keys.insert(contract_id.inner().to_repr(), vks);

    // We also have to initialize the Merkle trees used for coins.
    info!("Initializing Merkle trees");
    let mut faucet_merkle_tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(100);
    let mut alice_merkle_tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(100);

    // The faucet will now mint some tokens for Alice.
    let token_id = TokenId::from(pallas::Base::random(&mut OsRng));
    let amount = decode_base10("42.69", 8, true)?;

    info!("Building transfer tx for clear inputs");
    let (params, proofs, secret_keys) = build_transfer_tx(
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

    // Build transaction
    let mut data = vec![MoneyFunction::Transfer as u8];
    params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id, data }];
    let proofs = vec![proofs];
    let mut tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = tx.create_sigs(&mut OsRng, &secret_keys)?;
    tx.signatures = vec![sigs];

    // Get our ZK verifying keys in place for the tx verification
    let vks = verifying_keys.get(&contract_id.inner().to_repr()).unwrap();

    // Let's first execute this transaction for the faucet to see if it passes.
    // Then Alice gets the tx and also executes it.
    info!("Executing transaction on the faucet's blockchain db");
    verify_transaction(&faucet_blockchain, vks, &tx)?;
    info!("Adding coin to faucet's Merkle tree");
    faucet_merkle_tree.append(&MerkleNode::from(params.outputs[0].coin));

    info!("Executing transaction on Alice's blockchain db");
    verify_transaction(&alice_blockchain, vks, &tx)?;
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
    let (params, proofs, secret_keys) = build_transfer_tx(
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
    verify_transaction(&faucet_blockchain, vks, &tx)?;
    info!("Adding coin to faucet's Merkle tree");
    faucet_merkle_tree.append(&MerkleNode::from(params.outputs[0].coin));

    info!("Executing transaction on Alice's blockchain db");
    verify_transaction(&alice_blockchain, vks, &tx)?;
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
    let (params, proofs, secret_keys) = build_transfer_tx(
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
    verify_transaction(&faucet_blockchain, vks, &tx)?;
    info!("Adding coin to faucet's Merkle tree");
    faucet_merkle_tree.append(&MerkleNode::from(params.outputs[0].coin));
    info!("Executing transaction on Alice's blockchain db");
    verify_transaction(&alice_blockchain, vks, &tx)?;
    // TODO: FIXME: Actually have a look at the `merkle_add` calls
    alice_merkle_tree.append(&MerkleNode::from(params.outputs[0].coin));

    Ok(())
}

fn verify_transaction(
    blockchain: &Blockchain,
    verifying_keys: &[(String, VerifyingKey)],
    tx: &Transaction,
) -> Result<()> {
    info!("Begin transcation verification");
    // Table of public inputs used for ZK proof verification
    let mut zkp_table = vec![];
    // Table of public keys used for signature verification
    let mut sig_table = vec![];
    // State updates produced by contract execution
    let mut updates = vec![];

    // Iterate over all calls to get the metadata
    for (idx, call) in tx.calls.iter().enumerate() {
        info!("Verifying contract call {}", idx);
        let bincode = blockchain.wasm_bincode.get(call.contract_id)?;
        info!("Found wasm bincode for {}", call.contract_id);

        // Write the actual payload data
        let mut payload = vec![];
        payload.write_u32(idx as u32)?; // Call index
        tx.calls.encode(&mut payload)?; // Actual call_data

        // Instantiate the wasm runtime
        let mut runtime = Runtime::new(&bincode, blockchain.clone(), call.contract_id)?;
        info!("Executing \"metadata\" call");
        let metadata = runtime.metadata(&payload)?;
        let mut decoder = Cursor::new(&metadata);
        let zkp_pub: Vec<(String, Vec<pallas::Base>)> = Decodable::decode(&mut decoder)?;
        let sig_pub: Vec<PublicKey> = Decodable::decode(&mut decoder)?;
        zkp_table.push(zkp_pub);
        sig_table.push(sig_pub);
        info!("Successfully executed \"metadata\" call");

        info!("Executing \"exec\" call");
        let update = runtime.exec(&payload)?;
        updates.push(update);
        info!("Successfully executed \"exec\" call");
    }

    info!("Verifying transaction signatures");
    tx.verify_sigs(sig_table)?;
    info!("Signatures verified successfully");

    info!("Verifying transaction ZK proofs");
    tx.verify_zkps(verifying_keys, zkp_table)?;
    info!("Transaction ZK proofs verified successfully");

    // After the verification stage has passed, just apply all the changes.
    info!("Performing state updates");
    assert!(tx.calls.len() == updates.len());
    for (call, update) in tx.calls.iter().zip(updates.iter()) {
        let bincode = blockchain.wasm_bincode.get(call.contract_id)?;
        let mut runtime = Runtime::new(&bincode, blockchain.clone(), call.contract_id)?;
        info!("Executing \"apply\" call");
        runtime.apply(&update)?;
        info!("Successfully executed \"apply\" call");
    }

    info!("Transaction verified successfully");
    Ok(())
}
