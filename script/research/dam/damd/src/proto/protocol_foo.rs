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

use std::sync::Arc;

use async_trait::async_trait;
use tinyjson::JsonValue;
use tracing::{debug, error, info};

use darkfi::{
    impl_p2p_message,
    net::{
        metering::MeteringConfiguration,
        protocol::protocol_generic::{
            ProtocolGenericAction, ProtocolGenericHandler, ProtocolGenericHandlerPtr,
        },
        session::SESSION_DEFAULT,
        Message, P2pPtr,
    },
    rpc::jsonrpc::JsonSubscriber,
    system::ExecutorPtr,
    util::time::NanoTimestamp,
    Error, Result,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

/// Structure represening a foo request.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct FooRequest {
    /// Request message
    pub message: String,
}

impl_p2p_message!(
    FooRequest,
    "foorequest",
    0,
    0,
    MeteringConfiguration { threshold: 0, sleep_step: 0, expiry_time: NanoTimestamp::from_secs(0) }
);

/// Structure representing the response to `FooRequest`.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct FooResponse {
    /// Response code
    pub code: u8,
}

impl_p2p_message!(
    FooResponse,
    "fooresponse",
    0,
    0,
    MeteringConfiguration { threshold: 0, sleep_step: 0, expiry_time: NanoTimestamp::from_secs(0) }
);

/// Atomic pointer to the `ProtocolFoo` handler.
pub type ProtocolFooHandlerPtr = Arc<ProtocolFooHandler>;

/// Handler managing all `ProtocolFoo` messages, over generic P2P protocols.
pub struct ProtocolFooHandler {
    /// The generic handler for `FooRequest` messages.
    handler: ProtocolGenericHandlerPtr<FooRequest, FooResponse>,
}

impl ProtocolFooHandler {
    /// Initialize the generic prototocol handlers for all `ProtocolFoo` messages
    /// and register them to the provided P2P network, using the default session flag.
    pub async fn init(p2p: &P2pPtr) -> ProtocolFooHandlerPtr {
        debug!(
            target: "damd::proto::protocol_foo::init",
            "Adding all foo protocols to the protocol registry"
        );

        let handler = ProtocolGenericHandler::new(p2p, "ProtocolFoo", SESSION_DEFAULT).await;

        Arc::new(Self { handler })
    }

    /// Start all `ProtocolFoo` background tasks.
    pub async fn start(&self, executor: &ExecutorPtr, subscriber: JsonSubscriber) -> Result<()> {
        debug!(
            target: "damd::proto::protocol_foo::start",
            "Starting foo protocols handlers tasks..."
        );

        self.handler.task.clone().start(
            handle_receive_foo_request(self.handler.clone(), subscriber),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "damd::proto::protocol_foo::start", "Failed starting ProtocolFoo handler task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        debug!(
            target: "damd::proto::protocol_foo::start",
            "Foo protocols handlers tasks started!"
        );

        Ok(())
    }

    /// Stop all `ProtocolSync` background tasks.
    pub async fn stop(&self) {
        debug!(target: "damd::proto::protocol_foo::stop", "Terminating foo protocols handlers tasks...");
        self.handler.task.stop().await;
        debug!(target: "damd::proto::protocol_foo::stop", "Foo protocols handlers tasks terminated!");
    }
}

/// Background handler function for ProtocolFoo.
async fn handle_receive_foo_request(
    handler: ProtocolGenericHandlerPtr<FooRequest, FooResponse>,
    subscriber: JsonSubscriber,
) -> Result<()> {
    debug!(target: "damd::proto::protocol_foo::handle_receive_foo_request", "START");
    loop {
        // Wait for a new foo request message
        let (channel, request) = match handler.receiver.recv().await {
            Ok(r) => r,
            Err(e) => {
                debug!(
                    target: "damd::proto::protocol_foo::handle_receive_foo_request",
                    "recv fail: {e}"
                );
                continue
            }
        };

        let notification = format!("Received foo request from {channel}: {}", request.message);
        info!(target: "damd::proto::protocol_foo::handle_receive_foo_request", "{notification}");

        // Notify subscriber
        subscriber.notify(vec![JsonValue::String(notification)].into()).await;

        // Send response
        handler
            .send_action(channel, ProtocolGenericAction::Response(FooResponse { code: 42 }))
            .await;
    }
}
