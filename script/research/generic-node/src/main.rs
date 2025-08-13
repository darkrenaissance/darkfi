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

use std::{collections::HashSet, sync::Arc};

use rand::{rngs::OsRng, Rng};
use smol::{stream::StreamExt, Executor};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use tracing::{error, info};

use darkfi::{
    async_daemonize, cli_desc, impl_p2p_message,
    net::{
        metering::{MeteringConfiguration, DEFAULT_METERING_CONFIGURATION},
        protocol::protocol_generic::{
            ProtocolGenericAction, ProtocolGenericHandler, ProtocolGenericHandlerPtr,
        },
        session::SESSION_DEFAULT,
        settings::SettingsOpt,
        Message, P2p, P2pPtr, Settings,
    },
    system::{sleep, StoppableTask, StoppableTaskPtr},
    Error, Result,
};
use darkfi_serial::{async_trait, SerialDecodable, SerialEncodable};

const CONFIG_FILE: &str = "generic_node_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../generic_node_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "generic-node", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[structopt(short, long)]
    /// Node ID, used in the dummy messages
    node_id: u64,

    /// P2P network settings
    #[structopt(flatten)]
    net: SettingsOpt,
}

// Generic messages
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
struct GenericStringMessage {
    msg: String,
}
impl_p2p_message!(
    GenericStringMessage,
    "generic_string_message",
    0,
    0,
    DEFAULT_METERING_CONFIGURATION
);

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
struct GenericNumberMessage {
    num: u64,
}
impl_p2p_message!(
    GenericNumberMessage,
    "generic_number_message",
    0,
    0,
    DEFAULT_METERING_CONFIGURATION
);

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
struct GenericRequestMessage {
    msg: String,
}
impl_p2p_message!(
    GenericRequestMessage,
    "generic_request_message",
    0,
    0,
    DEFAULT_METERING_CONFIGURATION
);

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
struct GenericResponseMessage {
    msg: String,
}
impl_p2p_message!(
    GenericResponseMessage,
    "generic_response_message",
    0,
    0,
    DEFAULT_METERING_CONFIGURATION
);

/// Generic daemon structure
struct Genericd {
    /// Node ID, used in the dummy messages
    node_id: u64,
    /// P2P network pointer
    p2p: P2pPtr,
    /// GenericStringMessage handler
    generic_string_msg_handler:
        ProtocolGenericHandlerPtr<GenericStringMessage, GenericStringMessage>,
    /// GenericNumberMessage handler
    generic_number_msg_handler:
        ProtocolGenericHandlerPtr<GenericNumberMessage, GenericNumberMessage>,
    /// GenericRequestMessage handler
    generic_request_msg_handler:
        ProtocolGenericHandlerPtr<GenericRequestMessage, GenericResponseMessage>,
    /// Broadcasting messages task
    broadcast_task: StoppableTaskPtr,
}

impl Genericd {
    // Initialize daemon with all its required stuff.
    async fn new(
        node_id: u64,
        settings: &Settings,
        executor: &Arc<Executor<'static>>,
    ) -> Result<Self> {
        // Generating the p2p configuration and attaching our protocols
        let p2p = P2p::new(settings.clone(), executor.clone()).await?;

        // Add a generic protocol handler for GenericStringMessage
        let generic_string_msg_handler =
            ProtocolGenericHandler::new(&p2p, "ProtocolGenericString", SESSION_DEFAULT).await;

        // Add a generic protocol for GenericNumberMessage
        let generic_number_msg_handler =
            ProtocolGenericHandler::new(&p2p, "ProtocolGenericNumber", SESSION_DEFAULT).await;

        // Add a generic protocol for GenericRequestMessage
        let generic_request_msg_handler =
            ProtocolGenericHandler::new(&p2p, "ProtocolGenericRequest", SESSION_DEFAULT).await;

        let broadcast_task = StoppableTask::new();

        Ok(Self {
            node_id,
            p2p,
            generic_string_msg_handler,
            generic_number_msg_handler,
            generic_request_msg_handler,
            broadcast_task,
        })
    }

    /// Start all daemon background tasks.
    async fn start(&self) -> Result<()> {
        info!(target: "genericd", "Starting tasks...");

        self.generic_string_msg_handler.task.clone().start(
            handle_generic_string_msg(self.generic_string_msg_handler.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "genericd", "Failed starting protocol generic string handler task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            self.p2p.executor(),
        );

        self.generic_number_msg_handler.task.clone().start(
            handle_generic_number_msg(self.generic_number_msg_handler.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "genericd", "Failed starting protocol generic number handler task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            self.p2p.executor(),
        );

        self.generic_request_msg_handler.task.clone().start(
            handle_generic_request_msg(self.node_id, self.generic_request_msg_handler.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "genericd", "Failed starting protocol generic request handler task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            self.p2p.executor(),
        );

        self.p2p.clone().start().await?;

        self.broadcast_task.clone().start(
            broadcast_messages(self.node_id, self.p2p.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "genericd", "Failed starting broadcasting task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            self.p2p.executor(),
        );

        info!(target: "genericd", "All tasks started!");
        Ok(())
    }

    /// Stop all daemon background tasks.
    async fn stop(&self) {
        info!(target: "genericd", "Terminating tasks...");
        self.broadcast_task.stop().await;
        self.p2p.stop().await;
        self.generic_string_msg_handler.task.stop().await;
        self.generic_number_msg_handler.task.stop().await;
        self.generic_request_msg_handler.task.stop().await;
        info!(target: "genericd", "All tasks terminated!");
    }
}

/// Background handler function for GenericStringMessage.
async fn handle_generic_string_msg(
    handler: ProtocolGenericHandlerPtr<GenericStringMessage, GenericStringMessage>,
) -> Result<()> {
    let mut seen = HashSet::new();
    loop {
        // Wait for a new message
        let (channel, msg) = handler.receiver.recv().await?;

        if seen.contains(&msg.msg) {
            handler.send_action(channel, ProtocolGenericAction::Skip).await;
            continue
        }

        info!(target: "handle_generic_string_msg", "Received string message from channel {channel}: {}", msg.msg);
        seen.insert(msg.msg);

        handler.send_action(channel, ProtocolGenericAction::Broadcast).await;
    }
}

/// Background handler function for GenericNumberMessage.
async fn handle_generic_number_msg(
    handler: ProtocolGenericHandlerPtr<GenericNumberMessage, GenericNumberMessage>,
) -> Result<()> {
    let mut seen = HashSet::new();
    loop {
        // Wait for a new message
        let (channel, msg) = handler.receiver.recv().await?;

        if seen.contains(&msg.num) {
            handler.send_action(channel, ProtocolGenericAction::Skip).await;
            continue
        }

        info!(target: "handle_generic_number_msg", "Received number message from channel {channel}: {}", msg.num);
        seen.insert(msg.num);

        handler.send_action(channel, ProtocolGenericAction::Broadcast).await;
    }
}

/// Background handler function for GenericRequestMessage.
async fn handle_generic_request_msg(
    node_id: u64,
    handler: ProtocolGenericHandlerPtr<GenericRequestMessage, GenericResponseMessage>,
) -> Result<()> {
    let response = GenericResponseMessage { msg: format!("Pong from node {node_id}!") };
    loop {
        // Wait for a new message
        let (channel, msg) = handler.receiver.recv().await?;

        info!(target: "handle_generic_request_msg", "Received request message from channel {channel}: {}", msg.msg);

        handler.send_action(channel, ProtocolGenericAction::Response(response.clone())).await;
    }
}

/// Background function to send messages at random intervals.
async fn broadcast_messages(node_id: u64, p2p: P2pPtr) -> Result<()> {
    let comms_timeout = p2p.settings().read().await.outbound_connect_timeout_max();
    let request = GenericRequestMessage { msg: format!("Ping from node {node_id}!") };
    let mut counter = 0;
    loop {
        let sleep_time = OsRng.gen_range(1..=10);
        info!(target: "broadcast_messages", "Sleeping {sleep_time} till next broadcast...");
        sleep(sleep_time).await;

        info!(target: "broadcast_messages", "Broacasting messages...");
        // Broadcast a generic string message
        let string_msg =
            GenericStringMessage { msg: format!("Hello from node {node_id}({counter})!") };
        p2p.broadcast(&string_msg).await;

        // Broadcast a generic number message
        let number_msg = GenericNumberMessage { num: node_id + counter };
        p2p.broadcast(&number_msg).await;

        // Perform a direct request to each peer and grab their response
        let peers = p2p.hosts().channels();
        for peer in peers {
            info!(target: "broadcast_messages", "Sending request message to peer {peer:?}: {}", request.msg);
            let Ok(response_sub) = peer.subscribe_msg::<GenericResponseMessage>().await else {
                error!(target: "broadcast_messages", "Failure during `GenericResponseMessage` communication setup with peer: {peer:?}");
                continue
            };
            if let Err(e) = peer.send(&request).await {
                error!(target: "broadcast_messages", "Failure during `GenericResponseMessage` send to peer {peer:?}: {e}");
                continue
            };
            let Ok(response) = response_sub.receive_with_timeout(comms_timeout).await else {
                error!(target: "broadcast_messages", "Timeout while waiting for `GenericResponseMessage` from peer: {peer:?}");
                continue
            };
            info!(target: "broadcast_messages", "Received response message from peer {peer:?}: {}", response.msg);
        }

        counter += 1;
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    info!(target: "generic-node", "Initializing generic node...");

    let net_settings: Settings =
        (env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), args.net).try_into()?;
    let genericd = Genericd::new(args.node_id, &net_settings, &ex).await?;
    genericd.start().await?;

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "generic-node", "Caught termination signal, cleaning up and exiting...");

    info!(target: "generic-node", "Stopping genericd...");
    genericd.stop().await;

    Ok(())
}
