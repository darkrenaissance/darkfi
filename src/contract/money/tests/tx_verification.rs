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
    util::{parse::decode_base10, time::Timestamp},
    wallet::WalletDb,
    zk::{proof::ProvingKey, vm::ZkCircuit, vm_stack::empty_witnesses},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{constants::MERKLE_DEPTH, ContractId, Keypair, MerkleNode, TokenId},
    db::ZKAS_DB_NAME,
    incrementalmerkletree::bridgetree::BridgeTree,
    pasta::{
        group::ff::{Field, PrimeField},
        pallas,
    },
    tx::ContractCall,
};
use darkfi_serial::{serialize, Encodable};
use log::info;
use rand::rngs::OsRng;

use darkfi_money_contract::{client::build_transfer_tx, MoneyFunction, ZKAS_BURN_NS, ZKAS_MINT_NS};

/// Initialize log configuration
fn logger_init() -> Result<()> {
    let mut cfg = simplelog::ConfigBuilder::new();
    cfg.add_filter_ignore("sled".to_string());
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Info,
        cfg.build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )?;

    Ok(())
}

/// Generate N transactions
fn generate_txs(
    n: u64,
    faucet_kp: &Keypair,
    faucet_merkle_tree: &BridgeTree<MerkleNode, MERKLE_DEPTH>,
    contract_id: ContractId,
    mint_zkbin: &ZkBinary,
    mint_pk: &ProvingKey,
    burn_zkbin: &ZkBinary,
    burn_pk: &ProvingKey,
) -> Result<Vec<Transaction>> {
    let mut txs = vec![];
    for i in 0..n {
        // Generating dummy transaction
        info!("Generating transaction {}", i);
        let alice_kp = Keypair::random(&mut OsRng);
        let token_id = TokenId::from(pallas::Base::random(&mut OsRng));
        let amount = decode_base10("42.69", 8, true)?;

        let (params, proofs, secret_keys, _spent_coins) = build_transfer_tx(
            faucet_kp,
            &alice_kp.public,
            amount,
            token_id,
            &[],
            faucet_merkle_tree,
            mint_zkbin,
            mint_pk,
            burn_zkbin,
            burn_pk,
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

        txs.push(tx);
    }

    Ok(txs)
}

/// Check N faucet transactions verification performance
#[async_std::test]
async fn tx_faucet_verification() -> Result<()> {
    logger_init()?;

    // Test configuration
    let n = 10;

    // We initialize the faucet that will generate the transactions
    info!("Initializing faucet");
    let faucet_kp = Keypair::random(&mut OsRng);
    let faucet_pubkeys = vec![faucet_kp.public];
    let faucet_wallet = WalletDb::new("sqlite::memory:", "foo").await?;
    let faucet_sled_db = sled::Config::new().temporary(true).open()?;
    let faucet_state = ValidatorState::new(
        &faucet_sled_db,
        *TESTNET_GENESIS_TIMESTAMP,
        *TESTNET_GENESIS_HASH_BYTES,
        faucet_wallet,
        faucet_pubkeys.clone(),
        false,
    )
    .await?;

    info!("Looking up zkas circuits from DB");
    let contract_id = ContractId::from(pallas::Base::from(u64::MAX - 420));
    let faucet_sled = &faucet_state.read().await.blockchain.sled_db;
    let db_handle = faucet_state.read().await.blockchain.contracts.lookup(
        faucet_sled,
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

    info!("Initializing Merkle tree");
    let faucet_merkle_tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(100);

    // Generating transactions
    info!("Generating {} transactions", n);
    let init = Timestamp::current_time();
    let txs = generate_txs(
        n,
        &faucet_kp,
        &faucet_merkle_tree,
        contract_id,
        &mint_zkbin,
        &mint_pk,
        &burn_zkbin,
        &burn_pk,
    )?;
    let generation_elapsed_time = init.elapsed();
    assert_eq!(txs.len(), n as usize);

    // Verifying transactions
    info!("Verifying transactions...");
    let init = Timestamp::current_time();
    faucet_state.read().await.verify_transactions(&txs, true).await?;
    let verification_elapsed_time = init.elapsed();

    info!("Processing time of {} transactions(in sec):", n);
    info!("\tGeneration -> {}", generation_elapsed_time);
    info!("\tVerification -> {}", verification_elapsed_time);

    Ok(())
}
