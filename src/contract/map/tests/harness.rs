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
    runtime::vm_runtime::SMART_CONTRACT_ZKAS_DB_NAME,
    tx::Transaction,
    wallet::{WalletDb, WalletPtr},
    zk::{empty_witnesses, halo2::Field, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{Keypair, MerkleTree, PublicKey, SecretKey, DARK_TOKEN_ID, MAP_CONTRACT_ID},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};
use log::info;
use rand::rngs::OsRng;

use darkfi_map_contract::{
    model::SetParamsV1,
    client::set_v1::SetCallBuilder,
    ContractFunction,
};

pub const MAP_CONTRACT_ZKAS_SET_NS_V1: &str = "Set_V1";

pub fn init_logger() {
    let mut cfg = simplelog::ConfigBuilder::new();
    cfg.add_filter_ignore("sled".to_string());
    cfg.add_filter_ignore("blockchain::contractstore".to_string());

    // We check this error so we can execute same file tests in parallel,
    // otherwise second one fails to init logger here.
    if let Err(_) = simplelog::TermLogger::init(
        // simplelog::LevelFilter::Info,
        simplelog::LevelFilter::Debug,
        //simplelog::LevelFilter::Trace,
        cfg.build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    ) {
        info!(target: "map_harness", "Logger already initialized");
    }
}

pub struct Wallet {
    pub keypair: Keypair,
    pub state: ValidatorStatePtr,
    pub merkle_tree: MerkleTree,
    pub wallet: WalletPtr,
}

impl Wallet {
    async fn new(keypair: Keypair, faucet_pubkeys: &[PublicKey]) -> Result<Self> {
        let wallet = WalletDb::new("sqlite::memory:", "foo").await?;
        let sled_db = sled::Config::new().temporary(true).open()?;

        let state = ValidatorState::new(
            &sled_db,
            *TESTNET_BOOTSTRAP_TIMESTAMP,
            *TESTNET_GENESIS_TIMESTAMP,
            *TESTNET_GENESIS_HASH_BYTES,
            *TESTNET_INITIAL_DISTRIBUTION,
            wallet.clone(),
            faucet_pubkeys.to_vec(),
            false,
            false,
        )
        .await?;

        let merkle_tree = MerkleTree::new(100);

        Ok(Self { keypair, state, merkle_tree, wallet, })
    }
}


pub struct MapTestHarness {
    pub faucet: Wallet,
    pub alice: Wallet,
    pub proving_keys: HashMap<&'static str, (ProvingKey, ZkBinary)>,
}

impl MapTestHarness {
    pub async fn new() -> Result<Self> {
        let faucet_kp = Keypair::random(&mut OsRng);
        let faucet_pubkeys = vec![faucet_kp.public];
        let faucet = Wallet::new(faucet_kp, &faucet_pubkeys).await?;

        let alice_kp = Keypair::random(&mut OsRng);
        let alice = Wallet::new(alice_kp, &faucet_pubkeys).await?;

        // Get the zkas circuits and build proving keys
        let alice_sled = alice.state.read().await.blockchain.sled_db.clone();
        let db_handle = alice.state.read().await.blockchain.contracts.lookup(
            &alice_sled,
            &MAP_CONTRACT_ID,
            SMART_CONTRACT_ZKAS_DB_NAME,
        )?;

        // build proving keys
        let mut proving_keys = HashMap::new();
        macro_rules! mkpk {
            ($ns:expr) => {
                let zkas_bytes = db_handle.get(&serialize(&$ns))?.unwrap();
                let (zkbin, _): (Vec<u8>, Vec<u8>) = deserialize(&zkas_bytes)?;
                let zkbin = ZkBinary::decode(&zkbin)?;
                let witnesses = empty_witnesses(&zkbin);
                let circuit = ZkCircuit::new(witnesses, zkbin.clone());
                let pk = ProvingKey::build(13, &circuit);
                proving_keys.insert($ns, (pk, zkbin));
            };
        }
        mkpk!(MAP_CONTRACT_ZKAS_SET_NS_V1);

        Ok(Self { faucet, alice, proving_keys })
    }

    pub fn set(
        &self,
        secret: SecretKey,
        key: pallas::Base,
        value: pallas::Base,
    ) -> Result<(Transaction, SetParamsV1)> {
        let (prove_key, zkbin) = 
            self.proving_keys.get(&MAP_CONTRACT_ZKAS_SET_NS_V1).unwrap();   
        let debris = (SetCallBuilder {
            zkbin: zkbin.clone(),
            prove_key: prove_key.clone(),
            secret: secret.clone(),
            key: key.clone(),
            value: value.clone()
        }).build()?;

        let mut data = vec![ContractFunction::Set as u8];
        debris.params.encode(&mut data)?;
        let calls = vec![ContractCall { contract_id: *MAP_CONTRACT_ID, data: data }];
        let proofs = vec![debris.proofs];

        let mut tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = tx.create_sigs(&mut OsRng, &debris.signature_secrets)?;
        tx.signatures = vec![sigs];

        Ok((tx, debris.params))
    }
}

