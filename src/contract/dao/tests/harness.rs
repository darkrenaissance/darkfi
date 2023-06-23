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
    consensus::{TESTNET_GENESIS_HASH_BYTES, TESTNET_GENESIS_TIMESTAMP},
    runtime::vm_runtime::SMART_CONTRACT_ZKAS_DB_NAME,
    util::time::TimeKeeper,
    validator::{Validator, ValidatorConfig, ValidatorPtr},
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::crypto::{
    pasta_prelude::*, ContractId, Keypair, DAO_CONTRACT_ID, MONEY_CONTRACT_ID,
};
use darkfi_serial::{deserialize, serialize};
use log::{info, warn};
use rand::rngs::OsRng;

use darkfi_money_contract::{MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1};

use darkfi_dao_contract::{
    DAO_CONTRACT_ZKAS_DAO_EXEC_NS, DAO_CONTRACT_ZKAS_DAO_MINT_NS,
    DAO_CONTRACT_ZKAS_DAO_PROPOSE_BURN_NS, DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS,
    DAO_CONTRACT_ZKAS_DAO_VOTE_BURN_NS, DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS,
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

pub struct DaoTestHarness {
    /// Minting all new coins
    pub faucet_kp: Keypair,
    /// Governance token holder 1
    pub alice_kp: Keypair,
    /// Governance token holder 2
    pub bob_kp: Keypair,
    /// Governance token holder 3
    pub charlie_kp: Keypair,
    /// Receiver for treasury tokens
    pub rachel_kp: Keypair,
    /// DAO keypair
    pub dao_kp: Keypair,

    pub alice_validator: ValidatorPtr,
    pub money_contract_id: ContractId,
    pub dao_contract_id: ContractId,
    pub proving_keys: HashMap<[u8; 32], Vec<(&'static str, ProvingKey)>>,

    pub money_mint_zkbin: ZkBinary,
    pub money_mint_pk: ProvingKey,

    pub money_burn_zkbin: ZkBinary,
    pub money_burn_pk: ProvingKey,

    pub dao_mint_zkbin: ZkBinary,
    pub dao_mint_pk: ProvingKey,

    pub dao_propose_burn_zkbin: ZkBinary,
    pub dao_propose_burn_pk: ProvingKey,

    pub dao_propose_main_zkbin: ZkBinary,
    pub dao_propose_main_pk: ProvingKey,

    pub dao_vote_burn_zkbin: ZkBinary,
    pub dao_vote_burn_pk: ProvingKey,

    pub dao_vote_main_zkbin: ZkBinary,
    pub dao_vote_main_pk: ProvingKey,

    pub dao_exec_zkbin: ZkBinary,
    pub dao_exec_pk: ProvingKey,
}

impl DaoTestHarness {
    pub async fn new() -> Result<Self> {
        let faucet_kp = Keypair::random(&mut OsRng);
        let alice_kp = Keypair::random(&mut OsRng);
        let bob_kp = Keypair::random(&mut OsRng);
        let charlie_kp = Keypair::random(&mut OsRng);
        let rachel_kp = Keypair::random(&mut OsRng);
        let dao_kp = Keypair::random(&mut OsRng);

        let faucet_pubkeys = vec![faucet_kp.public];

        let alice_sled_db = sled::Config::new().temporary(true).open()?;

        // NOTE: we are not using consensus constants here so we
        // don't get circular dependencies.
        let time_keeper = TimeKeeper::new(*TESTNET_GENESIS_TIMESTAMP, 10, 90, 0);
        let config =
            ValidatorConfig::new(time_keeper, *TESTNET_GENESIS_HASH_BYTES, faucet_pubkeys.to_vec());
        let alice_validator = Validator::new(&alice_sled_db, config).await?;

        let money_contract_id = *MONEY_CONTRACT_ID;
        let dao_contract_id = *DAO_CONTRACT_ID;

        let alice_sled = alice_validator.read().await.blockchain.sled_db.clone();
        let money_db_handle = alice_validator.read().await.blockchain.contracts.lookup(
            &alice_sled,
            &money_contract_id,
            SMART_CONTRACT_ZKAS_DB_NAME,
        )?;
        let dao_db_handle = alice_validator.read().await.blockchain.contracts.lookup(
            &alice_sled,
            &dao_contract_id,
            SMART_CONTRACT_ZKAS_DB_NAME,
        )?;

        info!(target: "dao", "Creating zk proving keys");

        macro_rules! mkpk {
            ($ns:expr, $db_handle:expr) => {{
                let zkas_bytes = $db_handle.get(&serialize(&$ns))?.unwrap();
                let (zkbin, _): (Vec<u8>, Vec<u8>) = deserialize(&zkas_bytes)?;
                let zkbin = ZkBinary::decode(&zkbin)?;
                let witnesses = empty_witnesses(&zkbin);
                let circuit = ZkCircuit::new(witnesses, zkbin.clone());
                (zkbin, ProvingKey::build(13, &circuit))
            }};
        }

        let (money_mint_zkbin, money_mint_pk) =
            mkpk!(MONEY_CONTRACT_ZKAS_MINT_NS_V1, money_db_handle);

        let (money_burn_zkbin, money_burn_pk) =
            mkpk!(MONEY_CONTRACT_ZKAS_BURN_NS_V1, money_db_handle);

        let (dao_mint_zkbin, dao_mint_pk) = mkpk!(DAO_CONTRACT_ZKAS_DAO_MINT_NS, dao_db_handle);

        let (dao_propose_burn_zkbin, dao_propose_burn_pk) =
            mkpk!(DAO_CONTRACT_ZKAS_DAO_PROPOSE_BURN_NS, dao_db_handle);

        let (dao_propose_main_zkbin, dao_propose_main_pk) =
            mkpk!(DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS, dao_db_handle);

        let (dao_vote_burn_zkbin, dao_vote_burn_pk) =
            mkpk!(DAO_CONTRACT_ZKAS_DAO_VOTE_BURN_NS, dao_db_handle);

        let (dao_vote_main_zkbin, dao_vote_main_pk) =
            mkpk!(DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS, dao_db_handle);

        let (dao_exec_zkbin, dao_exec_pk) = mkpk!(DAO_CONTRACT_ZKAS_DAO_EXEC_NS, dao_db_handle);

        let mut proving_keys = HashMap::<[u8; 32], Vec<(&str, ProvingKey)>>::new();

        let pks = vec![
            (MONEY_CONTRACT_ZKAS_MINT_NS_V1, money_mint_pk.clone()),
            (MONEY_CONTRACT_ZKAS_BURN_NS_V1, money_burn_pk.clone()),
            (DAO_CONTRACT_ZKAS_DAO_MINT_NS, dao_mint_pk.clone()),
            (DAO_CONTRACT_ZKAS_DAO_PROPOSE_BURN_NS, dao_propose_burn_pk.clone()),
            (DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS, dao_propose_burn_pk.clone()),
            (DAO_CONTRACT_ZKAS_DAO_VOTE_BURN_NS, dao_propose_burn_pk.clone()),
            (DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS, dao_propose_burn_pk.clone()),
            (DAO_CONTRACT_ZKAS_DAO_EXEC_NS, dao_propose_burn_pk.clone()),
        ];
        proving_keys.insert(dao_contract_id.inner().to_repr(), pks);

        Ok(Self {
            faucet_kp,
            alice_kp,
            bob_kp,
            charlie_kp,
            rachel_kp,
            dao_kp,
            alice_validator,
            money_contract_id,
            dao_contract_id,
            proving_keys,
            money_mint_pk,
            money_mint_zkbin,
            money_burn_pk,
            money_burn_zkbin,
            dao_mint_zkbin,
            dao_mint_pk,
            dao_propose_burn_zkbin,
            dao_propose_burn_pk,
            dao_propose_main_zkbin,
            dao_propose_main_pk,
            dao_vote_burn_zkbin,
            dao_vote_burn_pk,
            dao_vote_main_zkbin,
            dao_vote_main_pk,
            dao_exec_zkbin,
            dao_exec_pk,
        })
    }
}
