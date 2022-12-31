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
        constants::{
            TESTNET_BOOTSTRAP_TIMESTAMP, TESTNET_GENESIS_HASH_BYTES, TESTNET_GENESIS_TIMESTAMP,
            TESTNET_INITIAL_DISTRIBUTION,
        },
        ValidatorState, ValidatorStatePtr,
    },
    wallet::WalletDb,
    zk::{proof::ProvingKey, vm::ZkCircuit, vm_stack::empty_witnesses},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{
        contract_id::{DAO_CONTRACT_ID, MONEY_CONTRACT_ID},
        ContractId, Keypair, MerkleTree,
    },
    db::SMART_CONTRACT_ZKAS_DB_NAME,
    pasta::group::ff::PrimeField,
};
use darkfi_serial::serialize;
use log::{info, warn};
use rand::rngs::OsRng;

use darkfi_money_contract::{MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1};

use darkfi_dao_contract::{
    DAO_CONTRACT_ZKAS_DAO_EXEC_NS, DAO_CONTRACT_ZKAS_DAO_MINT_NS,
    DAO_CONTRACT_ZKAS_DAO_PROPOSE_BURN_NS, DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS,
    DAO_CONTRACT_ZKAS_DAO_VOTE_BURN_NS, DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS,
};

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

    pub alice_state: ValidatorStatePtr,
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

        let alice_wallet = WalletDb::new("sqlite::memory:", "foo").await?;

        let alice_sled_db = sled::Config::new().temporary(true).open()?;

        let alice_state = ValidatorState::new(
            &alice_sled_db,
            *TESTNET_BOOTSTRAP_TIMESTAMP,
            *TESTNET_GENESIS_TIMESTAMP,
            *TESTNET_GENESIS_HASH_BYTES,
            *TESTNET_INITIAL_DISTRIBUTION,
            alice_wallet,
            faucet_pubkeys,
            false,
        )
        .await?;

        let money_contract_id = *MONEY_CONTRACT_ID;
        let dao_contract_id = *DAO_CONTRACT_ID;

        let alice_sled = alice_state.read().await.blockchain.sled_db.clone();
        let money_db_handle = alice_state.read().await.blockchain.contracts.lookup(
            &alice_sled,
            &money_contract_id,
            SMART_CONTRACT_ZKAS_DB_NAME,
        )?;
        let dao_db_handle = alice_state.read().await.blockchain.contracts.lookup(
            &alice_sled,
            &dao_contract_id,
            SMART_CONTRACT_ZKAS_DB_NAME,
        )?;

        info!("Decoding bincode");

        let money_mint_zkbin =
            money_db_handle.get(&serialize(&MONEY_CONTRACT_ZKAS_MINT_NS_V1))?.unwrap();
        let money_mint_zkbin = ZkBinary::decode(&money_mint_zkbin)?;
        let money_mint_witnesses = empty_witnesses(&money_mint_zkbin);
        let money_mint_circuit = ZkCircuit::new(money_mint_witnesses, money_mint_zkbin.clone());

        let money_burn_zkbin =
            money_db_handle.get(&serialize(&MONEY_CONTRACT_ZKAS_BURN_NS_V1))?.unwrap();
        let money_burn_zkbin = ZkBinary::decode(&money_burn_zkbin)?;
        let money_burn_witnesses = empty_witnesses(&money_burn_zkbin);
        let money_burn_circuit = ZkCircuit::new(money_burn_witnesses, money_burn_zkbin.clone());

        let dao_mint_zkbin =
            dao_db_handle.get(&serialize(&DAO_CONTRACT_ZKAS_DAO_MINT_NS))?.unwrap();
        let dao_mint_zkbin = ZkBinary::decode(&dao_mint_zkbin)?;
        let dao_mint_witnesses = empty_witnesses(&dao_mint_zkbin);
        let dao_mint_circuit = ZkCircuit::new(dao_mint_witnesses, dao_mint_zkbin.clone());

        let dao_propose_burn_zkbin =
            dao_db_handle.get(&serialize(&DAO_CONTRACT_ZKAS_DAO_PROPOSE_BURN_NS))?.unwrap();
        let dao_propose_burn_zkbin = ZkBinary::decode(&dao_propose_burn_zkbin)?;
        let dao_propose_burn_witnesses = empty_witnesses(&dao_propose_burn_zkbin);
        let dao_propose_burn_circuit =
            ZkCircuit::new(dao_propose_burn_witnesses, dao_propose_burn_zkbin.clone());

        let dao_propose_main_zkbin =
            dao_db_handle.get(&serialize(&DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS))?.unwrap();
        let dao_propose_main_zkbin = ZkBinary::decode(&dao_propose_main_zkbin)?;
        let dao_propose_main_witnesses = empty_witnesses(&dao_propose_main_zkbin);
        let dao_propose_main_circuit =
            ZkCircuit::new(dao_propose_main_witnesses, dao_propose_main_zkbin.clone());

        let dao_vote_burn_zkbin =
            dao_db_handle.get(&serialize(&DAO_CONTRACT_ZKAS_DAO_VOTE_BURN_NS))?.unwrap();
        let dao_vote_burn_zkbin = ZkBinary::decode(&dao_vote_burn_zkbin)?;
        let dao_vote_burn_witnesses = empty_witnesses(&dao_vote_burn_zkbin);
        let dao_vote_burn_circuit =
            ZkCircuit::new(dao_vote_burn_witnesses, dao_vote_burn_zkbin.clone());

        let dao_vote_main_zkbin =
            dao_db_handle.get(&serialize(&DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS))?.unwrap();
        let dao_vote_main_zkbin = ZkBinary::decode(&dao_vote_main_zkbin)?;
        let dao_vote_main_witnesses = empty_witnesses(&dao_vote_main_zkbin);
        let dao_vote_main_circuit =
            ZkCircuit::new(dao_vote_main_witnesses, dao_vote_main_zkbin.clone());

        let dao_exec_zkbin =
            dao_db_handle.get(&serialize(&DAO_CONTRACT_ZKAS_DAO_EXEC_NS))?.unwrap();
        let dao_exec_zkbin = ZkBinary::decode(&dao_exec_zkbin)?;
        let dao_exec_witnesses = empty_witnesses(&dao_exec_zkbin);
        let dao_exec_circuit = ZkCircuit::new(dao_exec_witnesses, dao_exec_zkbin.clone());

        info!("Creating zk proving keys");

        let k = 13;
        let mut proving_keys = HashMap::<[u8; 32], Vec<(&str, ProvingKey)>>::new();

        let money_mint_pk = ProvingKey::build(k, &money_mint_circuit);
        let money_burn_pk = ProvingKey::build(k, &money_burn_circuit);
        let dao_mint_pk = ProvingKey::build(k, &dao_mint_circuit);
        let dao_propose_burn_pk = ProvingKey::build(k, &dao_propose_burn_circuit);
        let dao_propose_main_pk = ProvingKey::build(k, &dao_propose_main_circuit);
        let dao_vote_burn_pk = ProvingKey::build(k, &dao_vote_burn_circuit);
        let dao_vote_main_pk = ProvingKey::build(k, &dao_vote_main_circuit);
        let dao_exec_pk = ProvingKey::build(k, &dao_exec_circuit);

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
            alice_state,
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
