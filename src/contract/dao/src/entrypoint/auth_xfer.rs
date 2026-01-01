/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use darkfi_money_contract::{
    error::MoneyError,
    model::{Coin, MoneyTransferParamsV1},
    MoneyFunction,
};
use darkfi_sdk::{
    crypto::{ContractId, FuncRef, PublicKey, DAO_CONTRACT_ID, MONEY_CONTRACT_ID},
    dark_tree::DarkLeaf,
    error::ContractError,
    msg,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, Encodable};

use crate::{
    error::DaoError,
    model::{DaoAuthCall, DaoAuthMoneyTransferParams, DaoExecParams, VecAuthCallCommit},
    DaoFunction, DAO_CONTRACT_ZKAS_AUTH_MONEY_TRANSFER_ENC_COIN_NS,
    DAO_CONTRACT_ZKAS_AUTH_MONEY_TRANSFER_NS,
};

/// `get_metdata` function for `Dao::AuthMoneyTransfer`
pub(crate) fn dao_authxfer_get_metadata(
    _cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx];
    let self_params: DaoAuthMoneyTransferParams = deserialize(&self_.data.data[1..])?;

    let sibling_idx = call_idx + 1;
    let xfer_call = &calls[sibling_idx].data;
    let xfer_params: MoneyTransferParamsV1 = deserialize(&xfer_call.data[1..])?;

    let parent_idx = calls[call_idx].parent_index.unwrap();
    let exec_callnode = &calls[parent_idx];
    let exec_params: DaoExecParams = deserialize(&exec_callnode.data.data[1..])?;

    if xfer_params.inputs.is_empty() {
        msg!("[Dao::AuthXfer] Error: Transfer inputs are missing");
        return Err(MoneyError::TransferMissingInputs.into())
    }

    if xfer_params.outputs.len() <= 1 {
        msg!("[Dao::AuthXfer] Error: Transfer outputs are missing");
        return Err(MoneyError::TransferMissingOutputs.into())
    }

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let signature_pubkeys: Vec<PublicKey> = vec![];

    for (output, enc_attrs) in xfer_params.outputs.iter().zip(self_params.enc_attrs.iter()) {
        let coin = output.coin;
        let (ephem_x, ephem_y) = enc_attrs.ephem_public.xy();
        zk_public_inputs.push((
            DAO_CONTRACT_ZKAS_AUTH_MONEY_TRANSFER_ENC_COIN_NS.to_string(),
            vec![
                coin.inner(),
                ephem_x,
                ephem_y,
                enc_attrs.encrypted_values[0],
                enc_attrs.encrypted_values[1],
                enc_attrs.encrypted_values[2],
                enc_attrs.encrypted_values[3],
                enc_attrs.encrypted_values[4],
            ],
        ));
    }

    // This value should be the same for all inputs, as enforced in process_instruction() below.
    let input_user_data_enc = xfer_params.inputs[0].user_data_enc;

    // Also check the coin in the change output
    let last_coin = xfer_params.outputs.last().unwrap().coin;

    let spend_hook =
        FuncRef { contract_id: *DAO_CONTRACT_ID, func_code: DaoFunction::Exec as u8 }.to_func_id();

    let (ephem_x, ephem_y) = self_params.dao_change_attrs.ephem_public.xy();
    zk_public_inputs.push((
        DAO_CONTRACT_ZKAS_AUTH_MONEY_TRANSFER_NS.to_string(),
        vec![
            exec_params.proposal_bulla.inner(),
            input_user_data_enc,
            last_coin.inner(),
            spend_hook.inner(),
            exec_params.proposal_auth_calls.commit(),
            ephem_x,
            ephem_y,
            self_params.dao_change_attrs.encrypted_values[0],
            self_params.dao_change_attrs.encrypted_values[1],
            self_params.dao_change_attrs.encrypted_values[2],
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

fn find_auth_in_parent(
    exec_callnode: &DarkLeaf<ContractCall>,
    proposal_auth_calls: Vec<DaoAuthCall>,
    self_call_idx: usize,
) -> Option<DaoAuthCall> {
    for (auth_call, child_idx) in
        proposal_auth_calls.into_iter().zip(exec_callnode.children_indexes.iter())
    {
        if *child_idx == self_call_idx {
            return Some(auth_call)
        }
    }
    None
}

/// `process_instruction` function for `Dao::AuthMoneyTransfer`
pub(crate) fn dao_authxfer_process_instruction(
    _cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let sibling_idx = call_idx + 1;
    let xfer_call = &calls[sibling_idx].data;

    ///////////////////////////////////////////////////
    // 1. Next call should be money transfer
    ///////////////////////////////////////////////////

    if xfer_call.contract_id != *MONEY_CONTRACT_ID {
        return Err(DaoError::AuthXferSiblingWrongContractId.into())
    }

    let xfer_call_function_code = xfer_call.data[0];
    if xfer_call_function_code != MoneyFunction::TransferV1 as u8 {
        return Err(DaoError::AuthXferSiblingWrongFunctionCode.into())
    }

    ///////////////////////////////////////////////////
    // 2. money::transfer() inputs should all have the same user_data
    ///////////////////////////////////////////////////

    let xfer_params: MoneyTransferParamsV1 = deserialize(&xfer_call.data[1..])?;
    if xfer_params.inputs.is_empty() {
        msg!("[Dao::AuthXfer] Error: Transfer inputs are missing");
        return Err(MoneyError::TransferMissingInputs.into())
    }

    // We need the last output to be the change
    if xfer_params.outputs.len() <= 1 {
        msg!("[Dao::AuthXfer] Error: Transfer outputs are missing");
        return Err(MoneyError::TransferMissingOutputs.into())
    }

    // MoneyTransfer should all have the same user_data set.
    // We check this by ensuring that user_data_enc is also the same for all inputs.
    // This means using the same blinding factor for all input's user_data.
    let user_data_enc = xfer_params.inputs[0].user_data_enc;
    for input in &xfer_params.inputs[1..] {
        if input.user_data_enc != user_data_enc {
            msg!("[Dao::Exec] Error: Money inputs unmatched user_data_enc");
            return Err(DaoError::AuthXferNonMatchingEncInputUserData.into())
        }
    }

    ///////////////////////////////////////////////////
    // 3. Check the coins on transfer outputs match
    ///////////////////////////////////////////////////

    // Find this auth_call in the parent DAO::exec()
    let parent_idx = calls[call_idx].parent_index.unwrap();
    let exec_callnode = &calls[parent_idx];
    let exec_params: DaoExecParams = deserialize(&exec_callnode.data.data[1..])?;

    let auth_call = find_auth_in_parent(exec_callnode, exec_params.proposal_auth_calls, call_idx);
    if auth_call.is_none() {
        return Err(DaoError::AuthXferCallNotFoundInParent.into())
    }

    // Read the proposal auth data which should be Vec<CoinAttributes>
    let proposal_coins: Vec<Coin> = deserialize(&auth_call.unwrap().auth_data[..])?;

    // Check all the outputs except the last match
    // There is the additional DAO change output which is always last.
    let outs = xfer_params.outputs;
    if outs.len() != proposal_coins.len() + 1 {
        return Err(DaoError::AuthXferWrongNumberOutputs.into())
    }
    for (output, coin) in outs.iter().zip(proposal_coins.iter()) {
        if output.coin != *coin {
            return Err(DaoError::AuthXferWrongOutputCoin.into())
        }
    }

    ///////////////////////////////////////////////////
    // 4. Change belongs to the DAO
    ///////////////////////////////////////////////////

    // The last output is sent back to the DAO. This is verified inside ZK.
    // Also the public_key should match.

    // We do not need to check the amounts, since sum(input values) == sum(output values)
    // otherwise the money::transfer() call is invalid.

    Ok(vec![])
}
