/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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
    blockchain::contract_store::SMART_CONTRACT_ZKAS_DB_NAME,
    zk::{empty_witnesses, ProvingKey, VerifyingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_dao_contract::{
    DAO_CONTRACT_ZKAS_DAO_AUTH_MONEY_TRANSFER_ENC_COIN_NS,
    DAO_CONTRACT_ZKAS_DAO_AUTH_MONEY_TRANSFER_NS, DAO_CONTRACT_ZKAS_DAO_EXEC_NS,
    DAO_CONTRACT_ZKAS_DAO_MINT_NS, DAO_CONTRACT_ZKAS_DAO_PROPOSE_INPUT_NS,
    DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS, DAO_CONTRACT_ZKAS_DAO_VOTE_INPUT_NS,
    DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS,
};
use darkfi_money_contract::{
    MONEY_CONTRACT_ZKAS_AUTH_TOKEN_MINT_NS_V1, MONEY_CONTRACT_ZKAS_BURN_NS_V1,
    MONEY_CONTRACT_ZKAS_FEE_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
    MONEY_CONTRACT_ZKAS_TOKEN_FRZ_NS_V1, MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1,
};
use darkfi_sdk::crypto::{DAO_CONTRACT_ID, MONEY_CONTRACT_ID};
use darkfi_serial::{deserialize, serialize};

use log::debug;

/// Update these if any circuits are changed.
/// Delete the existing cachefiles, and enable debug logging, you will see the new hashes.
const PKS_HASH: &str = "b19792d5c813eebe339af055fc3459cdad8e97e766a35a79f3e42964a2d6e610";
const VKS_HASH: &str = "9a92231ef4edfc20c2043d5255b558de5c07a9a4efda07718b689c299ac9440e";

/// Build a `PathBuf` to a cachefile
fn cache_path(typ: &str) -> Result<PathBuf> {
    let output = Command::new("git").arg("rev-parse").arg("--show-toplevel").output()?.stdout;
    let mut path = PathBuf::from(String::from_utf8(output[..output.len() - 1].to_vec())?);
    path.push("src");
    path.push("contract");
    path.push("test-harness");
    path.push(typ);
    Ok(path)
}

/// (Bincode, Namespace, VK)
pub type Vks = Vec<(Vec<u8>, String, Vec<u8>)>;
/// (Bincode, Namespace, VK)
pub type Pks = Vec<(Vec<u8>, String, Vec<u8>)>;

/// Generate or read cached PKs and VKs
pub fn get_cached_pks_and_vks() -> Result<(Pks, Vks)> {
    let pks_path = cache_path("pks.bin")?;
    let vks_path = cache_path("vks.bin")?;

    let mut pks = None;
    let mut vks = None;

    if pks_path.exists() {
        debug!("Found {:?}", pks_path);
        let mut f = File::open(pks_path.clone())?;
        let mut data = vec![];
        f.read_to_end(&mut data)?;

        let known_hash = blake3::Hash::from_hex(PKS_HASH)?;
        let found_hash = blake3::hash(&data);

        debug!("Known PKS hash: {}", known_hash);
        debug!("Found PKS hash: {}", found_hash);

        if known_hash == found_hash {
            pks = Some(deserialize(&data)?)
        }

        drop(f);
    }

    if vks_path.exists() {
        debug!("Found {:?}", vks_path);
        let mut f = File::open(vks_path.clone())?;
        let mut data = vec![];
        f.read_to_end(&mut data)?;

        let known_hash = blake3::Hash::from_hex(VKS_HASH)?;
        let found_hash = blake3::hash(&data);

        debug!("Known VKS hash: {}", known_hash);
        debug!("Found VKS hash: {}", found_hash);

        if known_hash == found_hash {
            vks = Some(deserialize(&data)?)
        }

        drop(f);
    }

    // Cache is correct, return
    if let (Some(pks), Some(vks)) = (pks, vks) {
        return Ok((pks, vks))
    }

    // Otherwise, build them
    let bins = vec![
        // Money
        &include_bytes!("../../money/proof/fee_v1.zk.bin")[..],
        &include_bytes!("../../money/proof/mint_v1.zk.bin")[..],
        &include_bytes!("../../money/proof/burn_v1.zk.bin")[..],
        &include_bytes!("../../money/proof/token_mint_v1.zk.bin")[..],
        &include_bytes!("../../money/proof/token_freeze_v1.zk.bin")[..],
        &include_bytes!("../../money/proof/auth_token_mint_v1.zk.bin")[..],
        // DAO
        &include_bytes!("../../dao/proof/mint.zk.bin")[..],
        &include_bytes!("../../dao/proof/propose-input.zk.bin")[..],
        &include_bytes!("../../dao/proof/propose-main.zk.bin")[..],
        &include_bytes!("../../dao/proof/vote-input.zk.bin")[..],
        &include_bytes!("../../dao/proof/vote-main.zk.bin")[..],
        &include_bytes!("../../dao/proof/exec.zk.bin")[..],
        &include_bytes!("../../dao/proof/auth-money-transfer.zk.bin")[..],
        &include_bytes!("../../dao/proof/auth-money-transfer-enc-coin.zk.bin")[..],
    ];

    let mut pks = vec![];
    let mut vks = vec![];

    for bincode in bins.iter() {
        let zkbin = ZkBinary::decode(bincode)?;
        debug!("Building PK for {}", zkbin.namespace);
        let witnesses = empty_witnesses(&zkbin)?;
        let circuit = ZkCircuit::new(witnesses, &zkbin);

        let pk = ProvingKey::build(zkbin.k, &circuit);
        let mut pk_buf = vec![];
        pk.write(&mut pk_buf)?;
        pks.push((bincode.to_vec(), zkbin.namespace.clone(), pk_buf));

        debug!("Building VK for {}", zkbin.namespace);
        let vk = VerifyingKey::build(zkbin.k, &circuit);
        let mut vk_buf = vec![];
        vk.write(&mut vk_buf)?;
        vks.push((bincode.to_vec(), zkbin.namespace.clone(), vk_buf));
    }

    debug!("Writing PKs to {:?}", pks_path);
    let mut f = File::create(&pks_path)?;
    let ser = serialize(&pks);
    let hash = blake3::hash(&ser);
    debug!("{:?} {}", pks_path, hash);
    f.write_all(&ser)?;

    debug!("Writing VKs to {:?}", vks_path);
    let mut f = File::create(&vks_path)?;
    let ser = serialize(&vks);
    let hash = blake3::hash(&ser);
    debug!("{:?} {}", vks_path, hash);
    f.write_all(&ser)?;

    Ok((pks, vks))
}

/// Inject cached VKs into a given blockchain database reference
pub fn inject(sled_db: &sled::Db, vks: &Vks) -> Result<()> {
    // Derive the database names for the specific contracts
    let money_db_name = MONEY_CONTRACT_ID.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME);
    let dao_db_name = DAO_CONTRACT_ID.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME);

    // Create the db trees
    let money_tree = sled_db.open_tree(money_db_name)?;
    let dao_tree = sled_db.open_tree(dao_db_name)?;

    for (bincode, namespace, vk) in vks.iter() {
        match namespace.as_str() {
            // Money contract circuits
            MONEY_CONTRACT_ZKAS_FEE_NS_V1 |
            MONEY_CONTRACT_ZKAS_MINT_NS_V1 |
            MONEY_CONTRACT_ZKAS_BURN_NS_V1 |
            MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1 |
            MONEY_CONTRACT_ZKAS_TOKEN_FRZ_NS_V1 |
            MONEY_CONTRACT_ZKAS_AUTH_TOKEN_MINT_NS_V1 => {
                let key = serialize(&namespace.as_str());
                let value = serialize(&(bincode.clone(), vk.clone()));
                money_tree.insert(key, value)?;
            }

            // DAO contract circuits
            DAO_CONTRACT_ZKAS_DAO_MINT_NS |
            DAO_CONTRACT_ZKAS_DAO_VOTE_INPUT_NS |
            DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS |
            DAO_CONTRACT_ZKAS_DAO_PROPOSE_INPUT_NS |
            DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS |
            DAO_CONTRACT_ZKAS_DAO_EXEC_NS |
            DAO_CONTRACT_ZKAS_DAO_AUTH_MONEY_TRANSFER_NS |
            DAO_CONTRACT_ZKAS_DAO_AUTH_MONEY_TRANSFER_ENC_COIN_NS => {
                let key = serialize(&namespace.as_str());
                let value = serialize(&(bincode.clone(), vk.clone()));
                dao_tree.insert(key, value)?;
            }

            x => panic!("Found unhandled zkas namespace {}", x),
        }
    }

    Ok(())
}
