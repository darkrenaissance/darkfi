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

use log::info;

use darkfi::{
    error::TxVerifyFailed,
    net::{P2p, P2pPtr, Settings, SESSION_ALL},
    tx::Transaction,
    validator::ValidatorPtr,
    Result,
};
use darkfi_consensus_contract::{
    model::ConsensusGenesisStakeParamsV1, ConsensusFunction::GenesisStakeV1,
};
use darkfi_money_contract::{model::MoneyTokenMintParamsV1, MoneyFunction::GenesisMintV1};
use darkfi_sdk::crypto::{CONSENSUS_CONTRACT_ID, MONEY_CONTRACT_ID};
use darkfi_serial::deserialize;

use crate::proto::{ProtocolBlock, ProtocolProposal, ProtocolSync, ProtocolTx};

/// Auxiliary function to calculate the total amount of minted tokens in provided
/// genesis transactions set. This includes both staked and normal tokens.
/// If a non-genesis transaction is found, execution fails.
pub fn genesis_txs_total(txs: &[Transaction]) -> Result<u64> {
    let mut total = 0;

    for tx in txs {
        // Transaction must contain a single Consensus::GenesisStake or Money::GenesisMint call
        if tx.calls.len() != 1 {
            return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
        }
        let call = &tx.calls[0];
        let function = call.data[0];
        if !(call.contract_id == *CONSENSUS_CONTRACT_ID || call.contract_id == *MONEY_CONTRACT_ID) ||
            (call.contract_id == *CONSENSUS_CONTRACT_ID && function != GenesisStakeV1 as u8) ||
            (call.contract_id == *MONEY_CONTRACT_ID && function != GenesisMintV1 as u8)
        {
            return Err(TxVerifyFailed::ErroneousTxs(vec![tx.clone()]).into())
        }

        let value = if function == GenesisStakeV1 as u8 {
            let params: ConsensusGenesisStakeParamsV1 = deserialize(&call.data[1..])?;
            params.input.value
        } else {
            let params: MoneyTokenMintParamsV1 = deserialize(&call.data[1..])?;
            params.input.value
        };

        total += value;
    }

    Ok(total)
}

/// Auxiliary function to generate the sync P2P network and register all its protocols.
pub async fn spawn_sync_p2p(settings: &Settings, validator: &ValidatorPtr) -> P2pPtr {
    info!(target: "darkfid", "Registering sync network P2P protocols...");
    let p2p = P2p::new(settings.clone()).await;
    let registry = p2p.protocol_registry();

    let _validator = validator.clone();
    registry
        .register(SESSION_ALL, move |channel, p2p| {
            let validator = _validator.clone();
            async move { ProtocolBlock::init(channel, validator, p2p).await.unwrap() }
        })
        .await;

    let _validator = validator.clone();
    registry
        .register(SESSION_ALL, move |channel, _p2p| {
            let validator = _validator.clone();
            async move { ProtocolSync::init(channel, validator).await.unwrap() }
        })
        .await;

    let _validator = validator.clone();
    registry
        .register(SESSION_ALL, move |channel, p2p| {
            let validator = _validator.clone();
            async move { ProtocolTx::init(channel, validator, p2p).await.unwrap() }
        })
        .await;

    p2p
}

/// Auxiliary function to generate the consensus P2P network and register all its protocols.
pub async fn spawn_consensus_p2p(settings: &Settings, validator: &ValidatorPtr) -> P2pPtr {
    info!(target: "darkfid", "Registering consensus network P2P protocols...");
    let p2p = P2p::new(settings.clone()).await;
    let registry = p2p.protocol_registry();

    let _validator = validator.clone();
    registry
        .register(SESSION_ALL, move |channel, p2p| {
            let validator = _validator.clone();
            async move { ProtocolProposal::init(channel, validator, p2p).await.unwrap() }
        })
        .await;

    p2p
}
