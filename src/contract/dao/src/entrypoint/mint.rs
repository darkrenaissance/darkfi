/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use darkfi_sdk::{
    crypto::{ContractId, MerkleNode, PublicKey},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::{
    error::DaoError,
    model::{DaoMintParams, DaoMintUpdate},
    DAO_CONTRACT_DB_DAO_BULLAS, DAO_CONTRACT_DB_DAO_MERKLE_ROOTS, DAO_CONTRACT_DB_INFO_TREE,
    DAO_CONTRACT_KEY_DAO_MERKLE_TREE, DAO_CONTRACT_KEY_LATEST_DAO_ROOT,
    DAO_CONTRACT_ZKAS_DAO_MINT_NS,
};

/// `get_metadata` function for `Dao::Mint`
pub(crate) fn dao_mint_get_metadata(
    _cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: DaoMintParams = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys: Vec<PublicKey> = vec![params.dao_pubkey];

    // In this Mint ZK proof, we constrain the DAO bulla and the signature pubkey
    let (pub_x, pub_y) = params.dao_pubkey.xy();

    zk_public_inputs.push((
        DAO_CONTRACT_ZKAS_DAO_MINT_NS.to_string(),
        vec![pub_x, pub_y, params.dao_bulla.inner()],
    ));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Dao::Mint`
pub(crate) fn dao_mint_process_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: DaoMintParams = deserialize(&self_.data[1..])?;

    // Check the DAO bulla doesn't already exist
    let bulla_db = wasm::db::db_lookup(cid, DAO_CONTRACT_DB_DAO_BULLAS)?;
    if wasm::db::db_contains_key(bulla_db, &serialize(&params.dao_bulla.inner()))? {
        msg!("[DAO::Mint] Error: DAO already exists {}", params.dao_bulla);
        return Err(DaoError::DaoAlreadyExists.into())
    }

    // Create state update
    let update = DaoMintUpdate { dao_bulla: params.dao_bulla };
    let mut update_data = vec![];
    update.encode(&mut update_data)?;

    Ok(update_data)
}

/// `process_update` function for `Dao::Mint`
pub(crate) fn dao_mint_process_update(cid: ContractId, update: DaoMintUpdate) -> ContractResult {
    // Grab all db handles we want to work on
    let info_db = wasm::db::db_lookup(cid, DAO_CONTRACT_DB_INFO_TREE)?;
    let bulla_db = wasm::db::db_lookup(cid, DAO_CONTRACT_DB_DAO_BULLAS)?;
    let roots_db = wasm::db::db_lookup(cid, DAO_CONTRACT_DB_DAO_MERKLE_ROOTS)?;

    wasm::db::db_set(bulla_db, &serialize(&update.dao_bulla), &[])?;

    let dao = vec![MerkleNode::from(update.dao_bulla.inner())];
    wasm::merkle::merkle_add(
        info_db,
        roots_db,
        DAO_CONTRACT_KEY_LATEST_DAO_ROOT,
        DAO_CONTRACT_KEY_DAO_MERKLE_TREE,
        &dao,
    )?;

    Ok(())
}
