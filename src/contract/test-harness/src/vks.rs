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

use std::{
    fs::File,
    io::{Read, Write},
    path::PathBuf,
    process::Command,
};

use darkfi::{
    runtime::vm_runtime::SMART_CONTRACT_ZKAS_DB_NAME,
    zk::{empty_witnesses, VerifyingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_dao_contract::{
    DAO_CONTRACT_ZKAS_DAO_EXEC_NS, DAO_CONTRACT_ZKAS_DAO_MINT_NS,
    DAO_CONTRACT_ZKAS_DAO_PROPOSE_BURN_NS, DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS,
    DAO_CONTRACT_ZKAS_DAO_VOTE_BURN_NS, DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS,
};
use darkfi_deployooor_contract::DEPLOY_CONTRACT_ZKAS_DERIVE_NS_V1;
use darkfi_money_contract::{
    CONSENSUS_CONTRACT_ZKAS_BURN_NS_V1, CONSENSUS_CONTRACT_ZKAS_MINT_NS_V1,
    CONSENSUS_CONTRACT_ZKAS_PROPOSAL_NS_V1, MONEY_CONTRACT_ZKAS_BURN_NS_V1,
    MONEY_CONTRACT_ZKAS_MINT_NS_V1, MONEY_CONTRACT_ZKAS_TOKEN_FRZ_NS_V1,
    MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1,
};
use darkfi_sdk::crypto::{
    contract_id::DEPLOYOOOR_CONTRACT_ID, CONSENSUS_CONTRACT_ID, DAO_CONTRACT_ID, MONEY_CONTRACT_ID,
};
use darkfi_serial::{deserialize, serialize};
use log::debug;

/// Update this if any circuits are changed
const VKS_HASH: &str = "0a745d4055440b52a9e669264a8cbf51a726d1ca290a4a3c6dc4cad3e711a1f3";

fn vks_path() -> Result<PathBuf> {
    let output = Command::new("git").arg("rev-parse").arg("--show-toplevel").output()?.stdout;
    let mut path = PathBuf::from(String::from_utf8(output[..output.len() - 1].to_vec())?);
    path.push("src");
    path.push("contract");
    path.push("test-harness");
    path.push("vks.bin");
    Ok(path)
}

/// (Bincode, Namespace, VK)
pub type Vks = Vec<(Vec<u8>, String, Vec<u8>)>;

fn read_or_gen_vks() -> Result<Vks> {
    let vks_path = vks_path()?;

    if vks_path.exists() {
        debug!("Found vks.bin");
        let mut f = File::open(vks_path.clone())?;
        let mut data = vec![];
        f.read_to_end(&mut data)?;

        let known_hash = blake3::Hash::from_hex(VKS_HASH)?;
        let found_hash = blake3::hash(&data);

        debug!("Known hash: {}", known_hash);
        debug!("Found hash: {}", found_hash);

        if known_hash == found_hash {
            return Ok(deserialize(&data)?)
        }

        drop(f);
    }

    let bins = vec![
        // Money
        &include_bytes!("../../money/proof/mint_v1.zk.bin")[..],
        &include_bytes!("../../money/proof/burn_v1.zk.bin")[..],
        &include_bytes!("../../money/proof/token_mint_v1.zk.bin")[..],
        &include_bytes!("../../money/proof/token_freeze_v1.zk.bin")[..],
        // DAO
        &include_bytes!("../../dao/proof/dao-mint.zk.bin")[..],
        &include_bytes!("../../dao/proof/dao-exec.zk.bin")[..],
        &include_bytes!("../../dao/proof/dao-propose-burn.zk.bin")[..],
        &include_bytes!("../../dao/proof/dao-propose-main.zk.bin")[..],
        &include_bytes!("../../dao/proof/dao-vote-burn.zk.bin")[..],
        &include_bytes!("../../dao/proof/dao-vote-main.zk.bin")[..],
        // Consensus
        &include_bytes!("../../consensus/proof/consensus_burn_v1.zk.bin")[..],
        &include_bytes!("../../consensus/proof/consensus_mint_v1.zk.bin")[..],
        &include_bytes!("../../consensus/proof/consensus_proposal_v1.zk.bin")[..],
        // Deployooor
        &include_bytes!("../../deployooor/proof/derive_contract_id.zk.bin")[..],
    ];

    let mut vks = vec![];

    for bincode in bins.iter() {
        let zkbin = ZkBinary::decode(&bincode)?;
        debug!("Building VK for {}", zkbin.namespace);
        let witnesses = empty_witnesses(&zkbin);
        let circuit = ZkCircuit::new(witnesses, zkbin.clone());
        let vk = VerifyingKey::build(13, &circuit);
        let mut vk_buf = vec![];
        vk.write(&mut vk_buf)?;
        vks.push((bincode.to_vec(), zkbin.namespace, vk_buf))
    }

    debug!("Writing to {:?}", vks_path);
    let mut f = File::create(vks_path)?;
    let ser = serialize(&vks);
    let hash = blake3::hash(&ser);
    debug!("vks.bin {}", hash);
    f.write_all(&ser)?;

    Ok(vks)
}

pub(crate) fn inject(sled_db: &sled::Db) -> Result<()> {
    // Use pregenerated vks
    let vks = read_or_gen_vks()?;

    // Inject them into the db
    let money_zkas_tree_ptr = MONEY_CONTRACT_ID.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME);
    let money_zkas_tree = sled_db.open_tree(money_zkas_tree_ptr)?;

    let dao_zkas_tree_ptr = DAO_CONTRACT_ID.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME);
    let dao_zkas_tree = sled_db.open_tree(dao_zkas_tree_ptr)?;

    let consensus_zkas_tree_ptr = CONSENSUS_CONTRACT_ID.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME);
    let consensus_zkas_tree = sled_db.open_tree(consensus_zkas_tree_ptr)?;

    let deployooor_zkas_tree_ptr =
        DEPLOYOOOR_CONTRACT_ID.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME);
    let deployooor_zkas_tree = sled_db.open_tree(deployooor_zkas_tree_ptr)?;

    for (bincode, namespace, vk) in vks.iter() {
        match namespace.as_str() {
            // Money circuits
            MONEY_CONTRACT_ZKAS_MINT_NS_V1 |
            MONEY_CONTRACT_ZKAS_BURN_NS_V1 |
            MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1 |
            MONEY_CONTRACT_ZKAS_TOKEN_FRZ_NS_V1 => {
                let key = serialize(&namespace.as_str());
                let value = serialize(&(bincode.clone(), vk.clone()));
                money_zkas_tree.insert(key, value)?;
            }

            // Deployooor circuits
            DEPLOY_CONTRACT_ZKAS_DERIVE_NS_V1 => {
                let key = serialize(&namespace.as_str());
                let value = serialize(&(bincode.clone(), vk.clone()));
                deployooor_zkas_tree.insert(key, value)?;
            }

            // DAO circuits
            DAO_CONTRACT_ZKAS_DAO_MINT_NS |
            DAO_CONTRACT_ZKAS_DAO_EXEC_NS |
            DAO_CONTRACT_ZKAS_DAO_VOTE_BURN_NS |
            DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS |
            DAO_CONTRACT_ZKAS_DAO_PROPOSE_BURN_NS |
            DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS => {
                let key = serialize(&namespace.as_str());
                let value = serialize(&(bincode.clone(), vk.clone()));
                dao_zkas_tree.insert(key, value)?;
            }

            // Consensus circuits
            CONSENSUS_CONTRACT_ZKAS_MINT_NS_V1 |
            CONSENSUS_CONTRACT_ZKAS_BURN_NS_V1 |
            CONSENSUS_CONTRACT_ZKAS_PROPOSAL_NS_V1 => {
                let key = serialize(&namespace.as_str());
                let value = serialize(&(bincode.clone(), vk.clone()));
                consensus_zkas_tree.insert(key, value)?;
            }

            x => panic!("Found unhandled zkas namespace {}", x),
        }
    }

    Ok(())
}
