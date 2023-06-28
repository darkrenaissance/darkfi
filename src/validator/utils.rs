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

use darkfi_sdk::crypto::{PublicKey, CONSENSUS_CONTRACT_ID, DAO_CONTRACT_ID, MONEY_CONTRACT_ID};
use darkfi_serial::serialize;
use log::info;

use crate::{
    blockchain::BlockchainOverlayPtr, runtime::vm_runtime::Runtime, util::time::TimeKeeper, Result,
};

/// Deploy DarkFi native wasm contracts to provided blockchain overlay.
/// If overlay already contains the contracts, it will just open the
/// necessary db and trees, and give back what it has. This means that
/// on subsequent runs, our native contracts will already be in a deployed
/// state, so what we actually do here is a redeployment. This kind of
/// operation should only modify the contract's state in case it wasn't
/// deployed before (meaning the initial run). Otherwise, it shouldn't
/// touch anything, or just potentially update the db schemas or whatever
/// is necessary. This logic should be handled in the init function of
/// the actual contract, so make sure the native contracts handle this well.
pub fn deploy_native_contracts(
    blockchain_overlay: BlockchainOverlayPtr,
    time_keeper: &TimeKeeper,
    faucet_pubkeys: &Vec<PublicKey>,
) -> Result<()> {
    info!(target: "validator", "Deploying native WASM contracts");

    // The faucet pubkeys are pubkeys which are allowed to create clear inputs
    // in the Money contract.
    let money_contract_deploy_payload = serialize(faucet_pubkeys);

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

    for nc in native_contracts {
        info!(target: "validator", "Deploying {} with ContractID {}", nc.0, nc.1);

        let mut runtime =
            Runtime::new(&nc.2[..], blockchain_overlay.clone(), nc.1, time_keeper.clone())?;

        runtime.deploy(&nc.3)?;

        info!(target: "validator", "Successfully deployed {}", nc.0);
    }

    info!(target: "validator", "Finished deployment of native WASM contracts");

    Ok(())
}
