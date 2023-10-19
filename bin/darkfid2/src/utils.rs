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

use std::{collections::HashMap, sync::Arc};

use log::info;
use smol::Executor;

use darkfi::{
    net::{P2p, P2pPtr, Settings, SESSION_ALL},
    rpc::jsonrpc::JsonSubscriber,
    validator::ValidatorPtr,
};

use crate::proto::{ProtocolBlock, ProtocolProposal, ProtocolSync, ProtocolTx};

/// Auxiliary function to generate the sync P2P network and register all its protocols.
pub async fn spawn_sync_p2p(
    settings: &Settings,
    validator: &ValidatorPtr,
    subscribers: &HashMap<&'static str, JsonSubscriber>,
    executor: Arc<Executor<'static>>,
) -> P2pPtr {
    info!(target: "darkfid", "Registering sync network P2P protocols...");
    let p2p = P2p::new(settings.clone(), executor.clone()).await;
    let registry = p2p.protocol_registry();

    let _validator = validator.clone();
    let _subscriber = subscribers.get("blocks").unwrap().clone();
    registry
        .register(SESSION_ALL, move |channel, p2p| {
            let validator = _validator.clone();
            let subscriber = _subscriber.clone();
            async move { ProtocolBlock::init(channel, validator, p2p, subscriber).await.unwrap() }
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
    let _subscriber = subscribers.get("txs").unwrap().clone();
    registry
        .register(SESSION_ALL, move |channel, p2p| {
            let validator = _validator.clone();
            let subscriber = _subscriber.clone();
            async move { ProtocolTx::init(channel, validator, p2p, subscriber).await.unwrap() }
        })
        .await;

    p2p
}

/// Auxiliary function to generate the consensus P2P network and register all its protocols.
pub async fn spawn_consensus_p2p(
    settings: &Settings,
    validator: &ValidatorPtr,
    subscribers: &HashMap<&'static str, JsonSubscriber>,
    executor: Arc<Executor<'static>>,
) -> P2pPtr {
    info!(target: "darkfid", "Registering consensus network P2P protocols...");
    let p2p = P2p::new(settings.clone(), executor.clone()).await;
    let registry = p2p.protocol_registry();

    let _validator = validator.clone();
    let _subscriber = subscribers.get("proposals").unwrap().clone();
    registry
        .register(SESSION_ALL, move |channel, p2p| {
            let validator = _validator.clone();
            let subscriber = _subscriber.clone();
            async move { ProtocolProposal::init(channel, validator, p2p, subscriber).await.unwrap() }
        })
        .await;

    p2p
}
