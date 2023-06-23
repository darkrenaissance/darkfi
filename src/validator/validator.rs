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

use async_std::sync::{Arc, RwLock};
use darkfi_sdk::crypto::{PublicKey, CONSENSUS_CONTRACT_ID, DAO_CONTRACT_ID, MONEY_CONTRACT_ID};
use darkfi_serial::serialize;
use log::info;

use crate::{
    blockchain::{Blockchain, BlockchainOverlay},
    runtime::vm_runtime::Runtime,
    util::time::Timestamp,
    Result,
};

use super::consensus::Consensus;

/// Atomic pointer to validator.
pub type ValidatorPtr = Arc<RwLock<Validator>>;

/// This struct represents a DarkFi validator node.
pub struct Validator {
    /// Canonical (finalized) blockchain
    pub blockchain: Blockchain,
    /// Hot/Live data used by the consensus algorithm
    pub consensus: Consensus,
}

/// Configuration for initializing [`Validator`]
pub struct ValidatorConfig {
    /// Genesis timestamp
    pub genesis_ts: Timestamp,
    /// Genesis data
    pub genesis_data: blake3::Hash,
    /// Whitelisted faucet pubkeys (testnet stuff)
    pub faucet_pubkeys: Vec<PublicKey>,
}

impl Validator {
    pub async fn new(db: &sled::Db, config: ValidatorConfig) -> Result<ValidatorPtr> {
        info!(target: "consensus::validator", "Initializing Validator");

        info!(target: "consensus::validator", "Initializing Blockchain");
        // TODO: Initialize chain, then check if its empty, so we can execute
        // the transactions of the genesis block
        let blockchain = Blockchain::new(db, config.genesis_ts, config.genesis_data)?;

        info!(target: "consensus::validator", "Initializing Consensus");
        let consensus = Consensus::new(blockchain.clone(), config.genesis_ts);

        // =====================
        // NATIVE WASM CONTRACTS
        // =====================
        // This is the current place where native contracts are being deployed.
        // When the `Blockchain` object is created, it doesn't care whether it
        // already has the contract data or not. If there's existing data, it
        // will just open the necessary db and trees, and give back what it has.
        // This means that on subsequent runs our native contracts will already
        // be in a deployed state, so what we actually do here is a redeployment.
        // This kind of operation should only modify the contract's state in case
        // it wasn't deployed before (meaning the initial run). Otherwise, it
        // shouldn't touch anything, or just potentially update the db schemas or
        // whatever is necessary. This logic should be handled in the init function
        // of the actual contract, so make sure the native contracts handle this well.

        // The faucet pubkeys are pubkeys which are allowed to create clear inputs
        // in the Money contract.
        let money_contract_deploy_payload = serialize(&config.faucet_pubkeys);

        // The DAO contract uses an empty payload to deploy itself.
        let dao_contract_deploy_payload = vec![];

        // The Consensus contract uses an empty payload to deploy itself.
        let consensus_contract_deploy_payload = vec![];

        let native_contracts = vec![
            (
                "Money Contract",
                *MONEY_CONTRACT_ID,
                include_bytes!("../contract/money/money_contract.wasm").to_vec(),
                money_contract_deploy_payload,
            ),
            (
                "DAO Contract",
                *DAO_CONTRACT_ID,
                include_bytes!("../contract/dao/dao_contract.wasm").to_vec(),
                dao_contract_deploy_payload,
            ),
            (
                "Consensus Contract",
                *CONSENSUS_CONTRACT_ID,
                include_bytes!("../contract/consensus/consensus_contract.wasm").to_vec(),
                consensus_contract_deploy_payload,
            ),
        ];

        info!(target: "consensus::validator", "Deploying native WASM contracts");
        let blockchain_overlay = BlockchainOverlay::new(&blockchain)?;

        for nc in native_contracts {
            info!(target: "consensus::validator", "Deploying {} with ContractID {}", nc.0, nc.1);

            let mut runtime = Runtime::new(
                &nc.2[..],
                blockchain_overlay.clone(),
                nc.1,
                consensus.time_keeper.clone(),
            )?;

            runtime.deploy(&nc.3)?;

            info!(target: "consensus::validator", "Successfully deployed {}", nc.0);
        }

        // Write the changes to the actual chain db
        blockchain_overlay.lock().unwrap().overlay.lock().unwrap().apply()?;

        info!(target: "consensus::validator", "Finished deployment of native WASM contracts");

        // Create the actual state
        let state = Arc::new(RwLock::new(Self { blockchain, consensus }));

        Ok(state)
    }
}
