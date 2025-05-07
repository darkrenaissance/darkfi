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

use std::sync::Arc;

use async_trait::async_trait;
use log::{debug, error};

use darkfi::{
    blockchain::{BlockInfo, Header, HeaderHash},
    impl_p2p_message,
    net::{
        metering::MeteringConfiguration,
        protocol::protocol_generic::{
            ProtocolGenericAction, ProtocolGenericHandler, ProtocolGenericHandlerPtr,
        },
        session::SESSION_DEFAULT,
        Message, P2pPtr,
    },
    system::ExecutorPtr,
    util::time::NanoTimestamp,
    validator::{consensus::Proposal, ValidatorPtr},
    Error, Result,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

// Constant defining max elements we send in vectors during syncing.
pub const BATCH: usize = 20;

// TODO: Fine tune
// Protocol metering configuration.
// Since all messages are synchronous(request -> response) we will define
// strict rules to prevent spamming.
// Each message score will be 1, with a threshold of 20 and expiry time of 5.
// Check ../tests/metering.rs for each message max bytes definition.
const PROTOCOL_SYNC_METERING_CONFIGURATION: MeteringConfiguration = MeteringConfiguration {
    threshold: 20,
    sleep_step: 500,
    expiry_time: NanoTimestamp::from_secs(5),
};

/// Structure represening a request to ask a node for their current
/// canonical(confirmed) tip block hash, if they are synced. We also
/// include our own tip, so they can verify we follow the same sequence.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct TipRequest {
    /// Canonical(confirmed) tip block hash
    pub tip: HeaderHash,
}

impl_p2p_message!(TipRequest, "tiprequest", 32, 1, PROTOCOL_SYNC_METERING_CONFIGURATION);

/// Structure representing the response to `TipRequest`,
/// containing a boolean flag to indicate if we are synced,
/// and our canonical(confirmed) tip block height and hash.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct TipResponse {
    /// Flag indicating the node is synced
    pub synced: bool,
    /// Canonical(confirmed) tip block height
    pub height: Option<u32>,
    /// Canonical(confirmed) tip block hash
    pub hash: Option<HeaderHash>,
}

impl_p2p_message!(TipResponse, "tipresponse", 39, 1, PROTOCOL_SYNC_METERING_CONFIGURATION);

/// Structure represening a request to ask a node for up to `BATCH` headers before
/// the provided header height.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct HeaderSyncRequest {
    /// Header height
    pub height: u32,
}

impl_p2p_message!(
    HeaderSyncRequest,
    "headersyncrequest",
    4,
    1,
    PROTOCOL_SYNC_METERING_CONFIGURATION
);

/// Structure representing the response to `HeaderSyncRequest`,
/// containing up to `BATCH` headers before the requested block height.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct HeaderSyncResponse {
    /// Response headers
    pub headers: Vec<Header>,
}

impl_p2p_message!(
    HeaderSyncResponse,
    "headersyncresponse",
    8192, // We leave some headroom for merge mining data
    1,
    PROTOCOL_SYNC_METERING_CONFIGURATION
);

/// Structure represening a request to ask a node for up to`BATCH` blocks
/// of provided headers.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct SyncRequest {
    /// Header hashes
    pub headers: Vec<HeaderHash>,
}

impl_p2p_message!(SyncRequest, "syncrequest", 641, 1, PROTOCOL_SYNC_METERING_CONFIGURATION);

/// Structure representing the response to `SyncRequest`,
/// containing up to `BATCH` blocks after the requested block height.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct SyncResponse {
    /// Response blocks
    pub blocks: Vec<BlockInfo>,
}

impl_p2p_message!(SyncResponse, "syncresponse", 0, 1, PROTOCOL_SYNC_METERING_CONFIGURATION);

/// Structure represening a request to ask a node a fork sequence.
/// If we include a specific fork tip, they have to return its sequence,
/// otherwise they respond with their best fork sequence.
/// We also include our own canonical(confirmed) tip, so they can verify
/// we follow the same sequence.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ForkSyncRequest {
    /// Canonical(confirmed) tip block hash
    pub tip: HeaderHash,
    /// Optional fork tip block hash
    pub fork_tip: Option<HeaderHash>,
}

impl_p2p_message!(ForkSyncRequest, "forksyncrequest", 65, 1, PROTOCOL_SYNC_METERING_CONFIGURATION);

/// Structure representing the response to `ForkSyncRequest`,
/// containing the requested fork sequence, up to `BATCH` proposals.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ForkSyncResponse {
    /// Response fork proposals
    pub proposals: Vec<Proposal>,
}

impl_p2p_message!(ForkSyncResponse, "forksyncresponse", 0, 1, PROTOCOL_SYNC_METERING_CONFIGURATION);

/// Structure represening a request to ask a node a fork header for the
/// requested height. The fork is identified by the provided header hash.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ForkHeaderHashRequest {
    /// Header height
    pub height: u32,
    /// Block header hash to identify the fork
    pub fork_header: HeaderHash,
}

impl_p2p_message!(
    ForkHeaderHashRequest,
    "forkheaderhashrequest",
    36,
    1,
    PROTOCOL_SYNC_METERING_CONFIGURATION
);

/// Structure representing the response to `ForkHeaderHashRequest`,
/// containing the requested fork header hash, if it was found.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ForkHeaderHashResponse {
    /// Response fork block header hash
    pub fork_header: Option<HeaderHash>,
}

impl_p2p_message!(
    ForkHeaderHashResponse,
    "forkheaderhashresponse",
    33,
    1,
    PROTOCOL_SYNC_METERING_CONFIGURATION
);

/// Structure represening a request to ask a node for up to `BATCH`
/// fork headers for provided header hashes.  The fork is identified
/// by the provided header hash.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ForkHeadersRequest {
    /// Header hashes
    pub headers: Vec<HeaderHash>,
    /// Block header hash to identify the fork
    pub fork_header: HeaderHash,
}

impl_p2p_message!(
    ForkHeadersRequest,
    "forkheadersrequest",
    673,
    1,
    PROTOCOL_SYNC_METERING_CONFIGURATION
);

/// Structure representing the response to `ForkHeadersRequest`,
/// containing up to `BATCH` fork headers.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ForkHeadersResponse {
    /// Response headers
    pub headers: Vec<Header>,
}

impl_p2p_message!(
    ForkHeadersResponse,
    "forkheadersresponse",
    8192, // We leave some headroom for merge mining data
    1,
    PROTOCOL_SYNC_METERING_CONFIGURATION
);

/// Structure represening a request to ask a node for up to `BATCH`
/// fork proposals for provided header hashes.  The fork is identified
/// by the provided header hash.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ForkProposalsRequest {
    /// Header hashes
    pub headers: Vec<HeaderHash>,
    /// Block header hash to identify the fork
    pub fork_header: HeaderHash,
}

impl_p2p_message!(
    ForkProposalsRequest,
    "forkproposalsrequest",
    673,
    1,
    PROTOCOL_SYNC_METERING_CONFIGURATION
);

/// Structure representing the response to `ForkProposalsRequest`,
/// containing up to `BATCH` fork headers.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ForkProposalsResponse {
    /// Response proposals
    pub proposals: Vec<Proposal>,
}

impl_p2p_message!(
    ForkProposalsResponse,
    "forkproposalsresponse",
    0,
    1,
    PROTOCOL_SYNC_METERING_CONFIGURATION
);

/// Atomic pointer to the `ProtocolSync` handler.
pub type ProtocolSyncHandlerPtr = Arc<ProtocolSyncHandler>;

/// Handler managing all `ProtocolSync` messages, over generic P2P protocols.
pub struct ProtocolSyncHandler {
    /// The generic handler for `TipRequest` messages.
    tip_handler: ProtocolGenericHandlerPtr<TipRequest, TipResponse>,
    /// The generic handler for `HeaderSyncRequest` messages.
    header_handler: ProtocolGenericHandlerPtr<HeaderSyncRequest, HeaderSyncResponse>,
    /// The generic handler for `SyncRequest` messages.
    sync_handler: ProtocolGenericHandlerPtr<SyncRequest, SyncResponse>,
    /// The generic handler for `ForkSyncRequest` messages.
    fork_sync_handler: ProtocolGenericHandlerPtr<ForkSyncRequest, ForkSyncResponse>,
    /// The generic handler for `ForkHeaderHashRequest` messages.
    fork_header_hash_handler:
        ProtocolGenericHandlerPtr<ForkHeaderHashRequest, ForkHeaderHashResponse>,
    /// The generic handler for `ForkHeadersRequest` messages.
    fork_headers_handler: ProtocolGenericHandlerPtr<ForkHeadersRequest, ForkHeadersResponse>,
    /// The generic handler for `ForkProposalsRequest` messages.
    fork_proposals_handler: ProtocolGenericHandlerPtr<ForkProposalsRequest, ForkProposalsResponse>,
}

impl ProtocolSyncHandler {
    /// Initialize the generic prototocol handlers for all `ProtocolSync` messages
    /// and register them to the provided P2P network, using the default session flag.
    pub async fn init(p2p: &P2pPtr) -> ProtocolSyncHandlerPtr {
        debug!(
            target: "darkfid::proto::protocol_sync::init",
            "Adding all sync protocols to the protocol registry"
        );

        let tip_handler =
            ProtocolGenericHandler::new(p2p, "ProtocolSyncTip", SESSION_DEFAULT).await;
        let header_handler =
            ProtocolGenericHandler::new(p2p, "ProtocolSyncHeader", SESSION_DEFAULT).await;
        let sync_handler = ProtocolGenericHandler::new(p2p, "ProtocolSync", SESSION_DEFAULT).await;
        let fork_sync_handler =
            ProtocolGenericHandler::new(p2p, "ProtocolSyncFork", SESSION_DEFAULT).await;
        let fork_header_hash_handler =
            ProtocolGenericHandler::new(p2p, "ProtocolSyncForkHeaderHash", SESSION_DEFAULT).await;
        let fork_headers_handler =
            ProtocolGenericHandler::new(p2p, "ProtocolSyncForkHeaders", SESSION_DEFAULT).await;
        let fork_proposals_handler =
            ProtocolGenericHandler::new(p2p, "ProtocolSyncForkProposals", SESSION_DEFAULT).await;

        Arc::new(Self {
            tip_handler,
            header_handler,
            sync_handler,
            fork_sync_handler,
            fork_header_hash_handler,
            fork_headers_handler,
            fork_proposals_handler,
        })
    }

    /// Start all `ProtocolSync` background tasks.
    pub async fn start(&self, executor: &ExecutorPtr, validator: &ValidatorPtr) -> Result<()> {
        debug!(
            target: "darkfid::proto::protocol_sync::start",
            "Starting sync protocols handlers tasks..."
        );

        self.tip_handler.task.clone().start(
            handle_receive_tip_request(self.tip_handler.clone(), validator.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "darkfid::proto::protocol_sync::start", "Failed starting ProtocolSyncTip handler task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        self.header_handler.task.clone().start(
            handle_receive_header_request(self.header_handler.clone(), validator.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "darkfid::proto::protocol_sync::start", "Failed starting ProtocolSyncHeader handler task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        self.sync_handler.task.clone().start(
            handle_receive_request(self.sync_handler.clone(), validator.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "darkfid::proto::protocol_sync::start", "Failed starting ProtocolSync handler task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        self.fork_sync_handler.task.clone().start(
            handle_receive_fork_request(self.fork_sync_handler.clone(), validator.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "darkfid::proto::protocol_sync::start", "Failed starting ProtocolSyncFork handler task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        self.fork_header_hash_handler.task.clone().start(
            handle_receive_fork_header_hash_request(self.fork_header_hash_handler.clone(), validator.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "darkfid::proto::protocol_sync::start", "Failed starting ProtocolSyncForkHeaderHash handler task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        self.fork_headers_handler.task.clone().start(
            handle_receive_fork_headers_request(self.fork_headers_handler.clone(), validator.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "darkfid::proto::protocol_sync::start", "Failed starting ProtocolSyncForkHeaders handler task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        self.fork_proposals_handler.task.clone().start(
            handle_receive_fork_proposals_request(self.fork_proposals_handler.clone(), validator.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "darkfid::proto::protocol_sync::start", "Failed starting ProtocolSyncForkProposals handler task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        debug!(
            target: "darkfid::proto::protocol_sync::start",
            "Sync protocols handlers tasks started!"
        );

        Ok(())
    }

    /// Stop all `ProtocolSync` background tasks.
    pub async fn stop(&self) {
        debug!(target: "darkfid::proto::protocol_sync::stop", "Terminating sync protocols handlers tasks...");
        self.tip_handler.task.stop().await;
        self.header_handler.task.stop().await;
        self.sync_handler.task.stop().await;
        self.fork_sync_handler.task.stop().await;
        self.fork_header_hash_handler.task.stop().await;
        self.fork_headers_handler.task.stop().await;
        self.fork_proposals_handler.task.stop().await;
        debug!(target: "darkfid::proto::protocol_sync::stop", "Sync protocols handlers tasks terminated!");
    }
}

/// Background handler function for ProtocolSyncTip.
async fn handle_receive_tip_request(
    handler: ProtocolGenericHandlerPtr<TipRequest, TipResponse>,
    validator: ValidatorPtr,
) -> Result<()> {
    debug!(target: "darkfid::proto::protocol_sync::handle_receive_tip_request", "START");
    loop {
        // Wait for a new tip request message
        let (channel, request) = match handler.receiver.recv().await {
            Ok(r) => r,
            Err(e) => {
                debug!(
                    target: "darkfid::proto::protocol_sync::handle_receive_tip_request",
                    "recv fail: {e}"
                );
                continue
            }
        };

        debug!(target: "darkfid::proto::protocol_sync::handle_receive_tip_request", "Received request: {request:?}");

        // Check if node has finished syncing its blockchain
        if !*validator.synced.read().await {
            debug!(
                target: "darkfid::proto::protocol_sync::handle_receive_tip_request",
                "Node still syncing blockchain"
            );
            handler
                .send_action(
                    channel,
                    ProtocolGenericAction::Response(TipResponse {
                        synced: false,
                        height: None,
                        hash: None,
                    }),
                )
                .await;
            continue
        }

        // Check we follow the same sequence
        match validator.blockchain.blocks.contains(&request.tip) {
            Ok(contains) => {
                if !contains {
                    debug!(
                        target: "darkfid::proto::protocol_sync::handle_receive_tip_request",
                        "Node doesn't follow request sequence"
                    );
                    handler.send_action(channel, ProtocolGenericAction::Skip).await;
                    continue
                }
            }
            Err(e) => {
                error!(
                    target: "darkfid::proto::protocol_sync::handle_receive_tip_request",
                    "block_store.contains fail: {e}"
                );
                handler.send_action(channel, ProtocolGenericAction::Skip).await;
                continue
            }
        }

        // Grab our current tip and return it
        let tip = match validator.blockchain.last() {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "darkfid::proto::protocol_sync::handle_receive_tip_request",
                    "blockchain.last fail: {e}"
                );
                handler.send_action(channel, ProtocolGenericAction::Skip).await;
                continue
            }
        };

        // Send response
        handler
            .send_action(
                channel,
                ProtocolGenericAction::Response(TipResponse {
                    synced: true,
                    height: Some(tip.0),
                    hash: Some(tip.1),
                }),
            )
            .await;
    }
}

/// Background handler function for ProtocolSyncHeader.
async fn handle_receive_header_request(
    handler: ProtocolGenericHandlerPtr<HeaderSyncRequest, HeaderSyncResponse>,
    validator: ValidatorPtr,
) -> Result<()> {
    debug!(target: "darkfid::proto::protocol_sync::handle_receive_header_request", "START");
    loop {
        // Wait for a new header request message
        let (channel, request) = match handler.receiver.recv().await {
            Ok(r) => r,
            Err(e) => {
                debug!(
                    target: "darkfid::proto::protocol_sync::handle_receive_header_request",
                    "recv fail: {e}"
                );
                continue
            }
        };

        // Check if node has finished syncing its blockchain
        if !*validator.synced.read().await {
            debug!(
                target: "darkfid::proto::protocol_sync::handle_receive_header_request",
                "Node still syncing blockchain, skipping..."
            );
            handler.send_action(channel, ProtocolGenericAction::Skip).await;
            continue
        }

        debug!(target: "darkfid::proto::protocol_sync::handle_receive_header_request", "Received request: {request:?}");

        // Grab the corresponding headers
        let headers = match validator.blockchain.get_headers_before(request.height, BATCH) {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "darkfid::proto::protocol_sync::handle_receive_header_request",
                    "get_headers_before fail: {}",
                    e
                );
                handler.send_action(channel, ProtocolGenericAction::Skip).await;
                continue
            }
        };

        // Send response
        handler
            .send_action(channel, ProtocolGenericAction::Response(HeaderSyncResponse { headers }))
            .await;
    }
}

/// Background handler function for ProtocolSync.
async fn handle_receive_request(
    handler: ProtocolGenericHandlerPtr<SyncRequest, SyncResponse>,
    validator: ValidatorPtr,
) -> Result<()> {
    debug!(target: "darkfid::proto::protocol_sync::handle_receive_request", "START");
    loop {
        // Wait for a new sync request message
        let (channel, request) = match handler.receiver.recv().await {
            Ok(r) => r,
            Err(e) => {
                debug!(
                    target: "darkfid::proto::protocol_sync::handle_receive_request",
                    "recv fail: {e}"
                );
                continue
            }
        };

        // Check if node has finished syncing its blockchain
        if !*validator.synced.read().await {
            debug!(
                target: "darkfid::proto::protocol_sync::handle_receive_request",
                "Node still syncing blockchain, skipping..."
            );
            handler.send_action(channel, ProtocolGenericAction::Skip).await;
            continue
        }

        // Check if request exists the configured limit
        if request.headers.len() > BATCH {
            debug!(
                target: "darkfid::proto::protocol_sync::handle_receive_request",
                "Node requested more blocks than allowed."
            );
            handler.send_action(channel, ProtocolGenericAction::Skip).await;
            continue
        }

        debug!(target: "darkfid::proto::protocol_sync::handle_receive_request", "Received request: {request:?}");

        // Grab the corresponding blocks
        let blocks = match validator.blockchain.get_blocks_by_hash(&request.headers) {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "darkfid::proto::protocol_sync::handle_receive_request",
                    "get_blocks_after fail: {}",
                    e
                );
                handler.send_action(channel, ProtocolGenericAction::Skip).await;
                continue
            }
        };

        // Send response
        handler
            .send_action(channel, ProtocolGenericAction::Response(SyncResponse { blocks }))
            .await;
    }
}

/// Background handler function for ProtocolSyncFork.
async fn handle_receive_fork_request(
    handler: ProtocolGenericHandlerPtr<ForkSyncRequest, ForkSyncResponse>,
    validator: ValidatorPtr,
) -> Result<()> {
    debug!(target: "darkfid::proto::protocol_sync::handle_receive_fork_request", "START");
    loop {
        // Wait for a new fork sync request message
        let (channel, request) = match handler.receiver.recv().await {
            Ok(r) => r,
            Err(e) => {
                debug!(
                    target: "darkfid::proto::protocol_sync::handle_receive_fork_request",
                    "recv fail: {e}"
                );
                continue
            }
        };

        // Check if node has finished syncing its blockchain
        if !*validator.synced.read().await {
            debug!(
                target: "darkfid::proto::protocol_sync::handle_receive_fork_request",
                "Node still syncing blockchain, skipping..."
            );
            handler.send_action(channel, ProtocolGenericAction::Skip).await;
            continue
        }

        debug!(target: "darkfid::proto::protocol_sync::handle_receive_fork_request", "Received request: {request:?}");

        // Retrieve proposals sequence
        let proposals = match validator
            .consensus
            .get_fork_proposals_after(request.tip, request.fork_tip, BATCH as u32)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                debug!(
                    target: "darkfid::proto::protocol_sync::handle_receive_fork_request",
                    "Getting fork proposals failed: {}",
                    e
                );
                handler.send_action(channel, ProtocolGenericAction::Skip).await;
                continue
            }
        };

        // Send response
        handler
            .send_action(channel, ProtocolGenericAction::Response(ForkSyncResponse { proposals }))
            .await;
    }
}

/// Background handler function for ProtocolSyncForkHeaderHash.
async fn handle_receive_fork_header_hash_request(
    handler: ProtocolGenericHandlerPtr<ForkHeaderHashRequest, ForkHeaderHashResponse>,
    validator: ValidatorPtr,
) -> Result<()> {
    debug!(target: "darkfid::proto::protocol_sync::handle_receive_fork_header_hash_request", "START");
    loop {
        // Wait for a new fork header hash request message
        let (channel, request) = match handler.receiver.recv().await {
            Ok(r) => r,
            Err(e) => {
                debug!(
                    target: "darkfid::proto::protocol_sync::handle_receive_fork_header_hash_request",
                    "recv fail: {e}"
                );
                continue
            }
        };

        // Check if node has finished syncing its blockchain
        if !*validator.synced.read().await {
            debug!(
                target: "darkfid::proto::protocol_sync::handle_receive_fork_header_hash_request",
                "Node still syncing blockchain, skipping..."
            );
            handler.send_action(channel, ProtocolGenericAction::Skip).await;
            continue
        }

        debug!(target: "darkfid::proto::protocol_sync::handle_receive_fork_header_hash_request", "Received request: {request:?}");

        // Retrieve fork header
        let fork_header = match validator
            .consensus
            .get_fork_header_hash(request.height, &request.fork_header)
            .await
        {
            Ok(h) => h,
            Err(e) => {
                debug!(
                    target: "darkfid::proto::protocol_sync::handle_receive_fork_header_hash_request",
                    "Getting fork header hash failed: {}",
                    e
                );
                handler.send_action(channel, ProtocolGenericAction::Skip).await;
                continue
            }
        };

        // Send response
        handler
            .send_action(
                channel,
                ProtocolGenericAction::Response(ForkHeaderHashResponse { fork_header }),
            )
            .await;
    }
}

/// Background handler function for ProtocolSyncForkHeaders.
async fn handle_receive_fork_headers_request(
    handler: ProtocolGenericHandlerPtr<ForkHeadersRequest, ForkHeadersResponse>,
    validator: ValidatorPtr,
) -> Result<()> {
    debug!(target: "darkfid::proto::protocol_sync::handle_receive_fork_headers_request", "START");
    loop {
        // Wait for a new fork header hash request message
        let (channel, request) = match handler.receiver.recv().await {
            Ok(r) => r,
            Err(e) => {
                debug!(
                    target: "darkfid::proto::protocol_sync::handle_receive_fork_headers_request",
                    "recv fail: {e}"
                );
                continue
            }
        };

        // Check if node has finished syncing its blockchain
        if !*validator.synced.read().await {
            debug!(
                target: "darkfid::proto::protocol_sync::handle_receive_fork_headers_request",
                "Node still syncing blockchain, skipping..."
            );
            handler.send_action(channel, ProtocolGenericAction::Skip).await;
            continue
        }

        // Check if request exists the configured limit
        if request.headers.len() > BATCH {
            debug!(
                target: "darkfid::proto::protocol_sync::handle_receive_fork_headers_request",
                "Node requested more headers than allowed."
            );
            handler.send_action(channel, ProtocolGenericAction::Skip).await;
            continue
        }

        debug!(target: "darkfid::proto::protocol_sync::handle_receive_fork_headers_request", "Received request: {request:?}");

        // Retrieve fork headers
        let headers = match validator
            .consensus
            .get_fork_headers(&request.headers, &request.fork_header)
            .await
        {
            Ok(h) => h,
            Err(e) => {
                debug!(
                    target: "darkfid::proto::protocol_sync::handle_receive_fork_headers_request",
                    "Getting fork headers failed: {}",
                    e
                );
                handler.send_action(channel, ProtocolGenericAction::Skip).await;
                continue
            }
        };

        // Send response
        handler
            .send_action(channel, ProtocolGenericAction::Response(ForkHeadersResponse { headers }))
            .await;
    }
}

/// Background handler function for ProtocolSyncForkProposals.
async fn handle_receive_fork_proposals_request(
    handler: ProtocolGenericHandlerPtr<ForkProposalsRequest, ForkProposalsResponse>,
    validator: ValidatorPtr,
) -> Result<()> {
    debug!(target: "darkfid::proto::protocol_sync::handle_receive_fork_proposals_request", "START");
    loop {
        // Wait for a new fork header hash request message
        let (channel, request) = match handler.receiver.recv().await {
            Ok(r) => r,
            Err(e) => {
                debug!(
                    target: "darkfid::proto::protocol_sync::handle_receive_fork_proposals_request",
                    "recv fail: {e}"
                );
                continue
            }
        };

        // Check if node has finished syncing its blockchain
        if !*validator.synced.read().await {
            debug!(
                target: "darkfid::proto::protocol_sync::handle_receive_fork_proposals_request",
                "Node still syncing blockchain, skipping..."
            );
            handler.send_action(channel, ProtocolGenericAction::Skip).await;
            continue
        }

        // Check if request exists the configured limit
        if request.headers.len() > BATCH {
            debug!(
                target: "darkfid::proto::protocol_sync::handle_receive_fork_proposals_request",
                "Node requested more proposals than allowed."
            );
            handler.send_action(channel, ProtocolGenericAction::Skip).await;
            continue
        }

        debug!(target: "darkfid::proto::protocol_sync::handle_receive_fork_proposals_request", "Received request: {request:?}");

        // Retrieve fork headers
        let proposals = match validator
            .consensus
            .get_fork_proposals(&request.headers, &request.fork_header)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                debug!(
                    target: "darkfid::proto::protocol_sync::handle_receive_fork_proposals_request",
                    "Getting fork proposals failed: {}",
                    e
                );
                handler.send_action(channel, ProtocolGenericAction::Skip).await;
                continue
            }
        };

        // Send response
        handler
            .send_action(
                channel,
                ProtocolGenericAction::Response(ForkProposalsResponse { proposals }),
            )
            .await;
    }
}
