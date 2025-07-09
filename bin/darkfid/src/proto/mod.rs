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

use std::{collections::HashMap, sync::Arc};

use darkfi::{
    net::{P2p, P2pPtr, Settings},
    rpc::jsonrpc::JsonSubscriber,
    system::ExecutorPtr,
    validator::ValidatorPtr,
    Result,
};
use tracing::info;

/// Block proposal broadcast protocol
mod protocol_proposal;
pub use protocol_proposal::{ProposalMessage, ProtocolProposalHandler, ProtocolProposalHandlerPtr};

/// Validator blockchain sync protocol
mod protocol_sync;
pub use protocol_sync::{
    ForkHeaderHashRequest, ForkHeaderHashResponse, ForkHeadersRequest, ForkHeadersResponse,
    ForkProposalsRequest, ForkProposalsResponse, ForkSyncRequest, ForkSyncResponse,
    HeaderSyncRequest, HeaderSyncResponse, ProtocolSyncHandler, ProtocolSyncHandlerPtr,
    SyncRequest, SyncResponse, TipRequest, TipResponse, BATCH,
};

/// Transaction broadcast protocol
mod protocol_tx;
pub use protocol_tx::{ProtocolTxHandler, ProtocolTxHandlerPtr};

/// Atomic pointer to the Darkfid P2P protocols handler.
pub type DarkfidP2pHandlerPtr = Arc<DarkfidP2pHandler>;

/// Darkfid P2P protocols handler.
pub struct DarkfidP2pHandler {
    /// P2P network pointer
    pub p2p: P2pPtr,
    /// `ProtocolProposal` messages handler
    proposals: ProtocolProposalHandlerPtr,
    /// `ProtocolSync` messages handler
    sync: ProtocolSyncHandlerPtr,
    /// `ProtocolTx` messages handler
    txs: ProtocolTxHandlerPtr,
}

impl DarkfidP2pHandler {
    /// Initialize a Darkfid P2P protocols handler.
    ///
    /// A new P2P instance is generated using provided settings and all
    /// corresponding protocols are registered.
    pub async fn init(settings: &Settings, executor: &ExecutorPtr) -> Result<DarkfidP2pHandlerPtr> {
        info!(
            target: "darkfid::proto::mod::DarkfidP2pHandler::init",
            "Initializing a new Darkfid P2P handler..."
        );

        // Generate a new P2P instance
        let p2p = P2p::new(settings.clone(), executor.clone()).await?;

        // Generate a new `ProtocolProposal` messages handler
        let proposals = ProtocolProposalHandler::init(&p2p).await;

        // Generate a new `ProtocolSync` messages handler
        let sync = ProtocolSyncHandler::init(&p2p).await;

        // Generate a new `ProtocolTx` messages handler
        let txs = ProtocolTxHandler::init(&p2p).await;

        info!(
            target: "darkfid::proto::mod::DarkfidP2pHandler::init",
            "Darkfid P2P handler generated successfully!"
        );

        Ok(Arc::new(Self { p2p, proposals, sync, txs }))
    }

    /// Start the Darkfid P2P protocols handler for provided validator.
    pub async fn start(
        &self,
        executor: &ExecutorPtr,
        validator: &ValidatorPtr,
        subscribers: &HashMap<&'static str, JsonSubscriber>,
    ) -> Result<()> {
        info!(
            target: "darkfid::proto::mod::DarkfidP2pHandler::start",
            "Starting the Darkfid P2P handler..."
        );

        // Start the `ProtocolProposal` messages handler
        let proposals_sub = subscribers.get("proposals").unwrap().clone();
        let blocks_sub = subscribers.get("blocks").unwrap().clone();
        self.proposals.start(executor, validator, &self.p2p, proposals_sub, blocks_sub).await?;

        // Start the `ProtocolSync` messages handler
        self.sync.start(executor, validator).await?;

        // Start the `ProtocolTx` messages handler
        let subscriber = subscribers.get("txs").unwrap().clone();
        self.txs.start(executor, validator, subscriber).await?;

        // Start the P2P instance
        self.p2p.clone().start().await?;

        info!(
            target: "darkfid::proto::mod::DarkfidP2pHandler::start",
            "Darkfid P2P handler started successfully!"
        );

        Ok(())
    }

    /// Stop the Darkfid P2P protocols handler.
    pub async fn stop(&self) {
        info!(target: "darkfid::proto::mod::DarkfidP2pHandler::stop", "Terminating Darkfid P2P handler...");

        // Stop the P2P instance
        self.p2p.stop().await;

        // Start the `ProtocolTx` messages handler
        self.txs.stop().await;

        // Start the `ProtocolSync` messages handler
        self.sync.stop().await;

        // Start the `ProtocolProposal` messages handler
        self.proposals.stop().await;

        info!(target: "darkfid::proto::mod::DarkfidP2pHandler::stop", "Darkfid P2P handler terminated successfully!");
    }
}
