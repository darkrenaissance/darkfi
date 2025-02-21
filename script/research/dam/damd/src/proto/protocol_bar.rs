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
use log::{debug, error, info};
use tinyjson::JsonValue;

use darkfi::{
    impl_p2p_message,
    net::{
        protocol::protocol_generic::{
            ProtocolGenericAction, ProtocolGenericHandler, ProtocolGenericHandlerPtr,
        },
        session::SESSION_DEFAULT,
        Message, P2pPtr,
    },
    rpc::jsonrpc::JsonSubscriber,
    system::ExecutorPtr,
    Error, Result,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

/// Structure represening a bar message
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct Bar {
    /// Bar message
    pub message: String,
}

impl_p2p_message!(Bar, "bar");

/// Atomic pointer to the `ProtocolBar` handler.
pub type ProtocolBarHandlerPtr = Arc<ProtocolBarHandler>;

/// Handler managing `ProtocolBar` messages, over a generic P2P protocol.
pub struct ProtocolBarHandler {
    /// The generic handler for `ProtocolBar` messages.
    handler: ProtocolGenericHandlerPtr<Bar, Bar>,
}

impl ProtocolBarHandler {
    /// Initialize a generic prototocol handler for `ProtocolBar` messages
    /// and registers it to the provided P2P network, using the default session flag.
    pub async fn init(p2p: &P2pPtr) -> ProtocolBarHandlerPtr {
        debug!(
            target: "damd::proto::protocol_bar::init",
            "Adding ProtocolBar to the protocol registry"
        );

        let handler = ProtocolGenericHandler::new(p2p, "ProtocolBar", SESSION_DEFAULT).await;

        Arc::new(Self { handler })
    }

    /// Start the `ProtocolBar` background task.
    pub async fn start(&self, executor: &ExecutorPtr, subscriber: JsonSubscriber) -> Result<()> {
        debug!(
            target: "damd::proto::protocol_bar::start",
            "Starting ProtocolBar handler task..."
        );

        self.handler.task.clone().start(
            handle_receive_bar(self.handler.clone(), subscriber),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "damd::proto::protocol_bar::start", "Failed starting ProtocolBar handler task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        debug!(
            target: "damd::proto::protocol_bar::start",
            "ProtocolBar handler task started!"
        );

        Ok(())
    }

    /// Stop the `ProtocolBar` background task.
    pub async fn stop(&self) {
        debug!(target: "damd::proto::protocol_bar::stop", "Terminating ProtocolBar handler task...");
        self.handler.task.stop().await;
        debug!(target: "damd::proto::protocol_bar::stop", "ProtocolBar handler task terminated!");
    }
}

/// Background handler function for ProtocolBar.
async fn handle_receive_bar(
    handler: ProtocolGenericHandlerPtr<Bar, Bar>,
    subscriber: JsonSubscriber,
) -> Result<()> {
    debug!(target: "damd::proto::protocol_bar::handle_receive_bar", "START");
    loop {
        // Wait for a new bar message
        let (channel, bar) = match handler.receiver.recv().await {
            Ok(r) => r,
            Err(e) => {
                debug!(
                    target: "damd::proto::protocol_bar::handle_receive_bar",
                    "recv fail: {e}"
                );
                continue
            }
        };

        let notification = format!("Received bar message from {channel}: {}", bar.message);
        info!(target: "damd::proto::protocol_bar::handle_receive_bar", "{notification}");

        // Notify subscriber
        subscriber.notify(vec![JsonValue::String(notification)].into()).await;

        // Signal handler to broadcast the message to rest nodes
        handler.send_action(channel, ProtocolGenericAction::Broadcast).await;
    }
}
