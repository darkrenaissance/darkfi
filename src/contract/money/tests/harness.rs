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
use std::collections::HashMap;

use darkfi::{
    consensus::{
        ValidatorState, ValidatorStatePtr, TESTNET_BOOTSTRAP_TIMESTAMP, TESTNET_GENESIS_HASH_BYTES,
        TESTNET_GENESIS_TIMESTAMP, TESTNET_INITIAL_DISTRIBUTION,
    },
    tx::Transaction,
    wallet::WalletDb,
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{
        pasta_prelude::*, ContractId, Keypair, MerkleTree, PublicKey, TokenId, MONEY_CONTRACT_ID,
    },
    db::SMART_CONTRACT_ZKAS_DB_NAME,
    ContractCall,
};
use darkfi_serial::{serialize, Encodable};
use log::{info, warn};
use rand::rngs::OsRng;

use darkfi_money_contract::{
    client::build_transfer_tx, state::MoneyTransferParams, MoneyFunction,
    MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};

pub fn init_logger() -> Result<()> {
    let mut cfg = simplelog::ConfigBuilder::new();
    cfg.add_filter_ignore("sled".to_string());
    if let Err(_) = simplelog::TermLogger::init(
        //simplelog::LevelFilter::Info,
        simplelog::LevelFilter::Debug,
        //simplelog::LevelFilter::Trace,
        cfg.build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    ) {
        warn!(target: "dao", "Logger already initialized");
    }

    Ok(())
}

pub struct MoneyTestHarness {
    pub faucet_kp: Keypair,
    pub alice_kp: Keypair,
    pub bob_kp: Keypair,
    pub charlie_kp: Keypair,
    pub faucet_pubkeys: Vec<PublicKey>,
    pub faucet_state: ValidatorStatePtr,
    pub alice_state: ValidatorStatePtr,
    pub bob_state: ValidatorStatePtr,
    pub charlie_state: ValidatorStatePtr,
    pub money_contract_id: ContractId,
    pub proving_keys: HashMap<[u8; 32], Vec<(&'static str, ProvingKey)>>,
    pub mint_zkbin: ZkBinary,
    pub burn_zkbin: ZkBinary,
    pub mint_pk: ProvingKey,
    pub burn_pk: ProvingKey,
    pub faucet_merkle_tree: MerkleTree,
    pub alice_merkle_tree: MerkleTree,
    pub bob_merkle_tree: MerkleTree,
    pub charlie_merkle_tree: MerkleTree,
}

impl MoneyTestHarness {
    pub async fn new() -> Result<Self> {
        let faucet_kp = Keypair::random(&mut OsRng);
        let alice_kp = Keypair::random(&mut OsRng);
        let bob_kp = Keypair::random(&mut OsRng);
        let charlie_kp = Keypair::random(&mut OsRng);
        let faucet_pubkeys = vec![faucet_kp.public];

        let faucet_wallet = WalletDb::new("sqlite::memory:", "foo").await?;
        let alice_wallet = WalletDb::new("sqlite::memory:", "foo").await?;
        let bob_wallet = WalletDb::new("sqlite::memory:", "foo").await?;
        let charlie_wallet = WalletDb::new("sqlite::memory:", "foo").await?;

        let faucet_sled_db = sled::Config::new().temporary(true).open()?;
        let alice_sled_db = sled::Config::new().temporary(true).open()?;
        let bob_sled_db = sled::Config::new().temporary(true).open()?;
        let charlie_sled_db = sled::Config::new().temporary(true).open()?;

        let faucet_state = ValidatorState::new(
            &faucet_sled_db,
            *TESTNET_BOOTSTRAP_TIMESTAMP,
            *TESTNET_GENESIS_TIMESTAMP,
            *TESTNET_GENESIS_HASH_BYTES,
            *TESTNET_INITIAL_DISTRIBUTION,
            faucet_wallet,
            faucet_pubkeys.clone(),
            false,
        )
        .await?;

        let alice_state = ValidatorState::new(
            &alice_sled_db,
            *TESTNET_BOOTSTRAP_TIMESTAMP,
            *TESTNET_GENESIS_TIMESTAMP,
            *TESTNET_GENESIS_HASH_BYTES,
            *TESTNET_INITIAL_DISTRIBUTION,
            alice_wallet,
            faucet_pubkeys.clone(),
            false,
        )
        .await?;

        let bob_state = ValidatorState::new(
            &bob_sled_db,
            *TESTNET_BOOTSTRAP_TIMESTAMP,
            *TESTNET_GENESIS_TIMESTAMP,
            *TESTNET_GENESIS_HASH_BYTES,
            *TESTNET_INITIAL_DISTRIBUTION,
            bob_wallet,
            faucet_pubkeys.clone(),
            false,
        )
        .await?;

        let charlie_state = ValidatorState::new(
            &charlie_sled_db,
            *TESTNET_BOOTSTRAP_TIMESTAMP,
            *TESTNET_GENESIS_TIMESTAMP,
            *TESTNET_GENESIS_HASH_BYTES,
            *TESTNET_INITIAL_DISTRIBUTION,
            charlie_wallet,
            faucet_pubkeys.clone(),
            false,
        )
        .await?;

        let money_contract_id = *MONEY_CONTRACT_ID;

        let alice_sled = alice_state.read().await.blockchain.sled_db.clone();
        let db_handle = alice_state.read().await.blockchain.contracts.lookup(
            &alice_sled,
            &money_contract_id,
            SMART_CONTRACT_ZKAS_DB_NAME,
        )?;

        let mint_zkbin = db_handle.get(&serialize(&MONEY_CONTRACT_ZKAS_MINT_NS_V1))?.unwrap();
        let burn_zkbin = db_handle.get(&serialize(&MONEY_CONTRACT_ZKAS_BURN_NS_V1))?.unwrap();
        info!(target: "dao", "Decoding bincode");
        let mint_zkbin = ZkBinary::decode(&mint_zkbin)?;
        let burn_zkbin = ZkBinary::decode(&burn_zkbin)?;
        let mint_witnesses = empty_witnesses(&mint_zkbin);
        let burn_witnesses = empty_witnesses(&burn_zkbin);
        let mint_circuit = ZkCircuit::new(mint_witnesses, mint_zkbin.clone());
        let burn_circuit = ZkCircuit::new(burn_witnesses, burn_zkbin.clone());

        info!(target: "dao", "Creating zk proving keys");
        let k = 13;
        let mut proving_keys = HashMap::<[u8; 32], Vec<(&str, ProvingKey)>>::new();
        let mint_pk = ProvingKey::build(k, &mint_circuit);
        let burn_pk = ProvingKey::build(k, &burn_circuit);
        let pks = vec![
            (MONEY_CONTRACT_ZKAS_MINT_NS_V1, mint_pk.clone()),
            (MONEY_CONTRACT_ZKAS_BURN_NS_V1, burn_pk.clone()),
        ];
        proving_keys.insert(money_contract_id.inner().to_repr(), pks);

        let faucet_merkle_tree = MerkleTree::new(100);
        let alice_merkle_tree = MerkleTree::new(100);
        let bob_merkle_tree = MerkleTree::new(100);
        let charlie_merkle_tree = MerkleTree::new(100);

        Ok(Self {
            faucet_kp,
            alice_kp,
            bob_kp,
            charlie_kp,
            faucet_pubkeys,
            faucet_state,
            alice_state,
            bob_state,
            charlie_state,
            money_contract_id,
            proving_keys,
            mint_pk,
            burn_pk,
            mint_zkbin,
            burn_zkbin,
            faucet_merkle_tree,
            alice_merkle_tree,
            bob_merkle_tree,
            charlie_merkle_tree,
        })
    }

    pub fn airdrop(
        &self,
        amount: u64,
        token_id: TokenId,
        rcpt: &PublicKey,
    ) -> Result<(Transaction, MoneyTransferParams)> {
        let (params, proofs, secret_keys, _) = build_transfer_tx(
            &self.faucet_kp,
            rcpt,
            amount,
            token_id,
            &[],
            &self.faucet_merkle_tree,
            &self.mint_zkbin,
            &self.mint_pk,
            &self.burn_zkbin,
            &self.burn_pk,
            true,
        )?;

        let contract_id = *MONEY_CONTRACT_ID;

        let mut data = vec![MoneyFunction::Transfer as u8];
        params.encode(&mut data)?;
        let calls = vec![ContractCall { contract_id, data }];
        let proofs = vec![proofs];
        let mut tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = tx.create_sigs(&mut OsRng, &secret_keys)?;
        tx.signatures = vec![sigs];

        Ok((tx, params))
    }
}
