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

use darkfi_money_contract::{model::MoneyTransferParamsV1, MoneyFunction};
use darkfi_sdk::{
    crypto::{contract_id::MONEY_CONTRACT_ID, pasta_prelude::*, ContractId, PublicKey},
    dark_tree::DarkLeaf,
    db::{db_del, db_get, db_lookup},
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    error::DaoError, model::DaoAuthMoneyTransferParams, DaoFunction,
    DAO_CONTRACT_DB_PROPOSAL_BULLAS, DAO_CONTRACT_ZKAS_DAO_EXEC_NS,
};

/// `get_metdata` function for `Dao::Exec`
pub(crate) fn dao_authxfer_get_metadata(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let signature_pubkeys: Vec<PublicKey> = vec![];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Dao::Exec`
pub(crate) fn dao_authxfer_process_instruction(
    cid: ContractId,
    call_idx: u32,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let mut update_data = vec![];
    update_data.write_u8(DaoFunction::AuthMoneyTransfer as u8)?;
    Ok(update_data)
}
