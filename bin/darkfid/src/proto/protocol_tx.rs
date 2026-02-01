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

use tinyjson::JsonValue;
use tracing::{debug, error};

use darkfi::{
    net::{
        protocol::protocol_generic::{
            ProtocolGenericAction, ProtocolGenericHandler, ProtocolGenericHandlerPtr,
        },
        session::SESSION_DEFAULT,
        P2pPtr,
    },
    rpc::jsonrpc::JsonSubscriber,
    system::ExecutorPtr,
    tx::Transaction,
    util::encoding::base64,
    validator::ValidatorPtr,
    Error, Result,
};
use darkfi_serial::serialize_async;

/// Atomic pointer to the `ProtocolTx` handler.
pub type ProtocolTxHandlerPtr = Arc<ProtocolTxHandler>;

/// Handler managing [`Transaction`] messages, over a generic P2P protocol.
pub struct ProtocolTxHandler {
    /// The generic handler for [`Transaction`] messages.
    handler: ProtocolGenericHandlerPtr<Transaction, Transaction>,
}

impl ProtocolTxHandler {
    /// Initialize a generic prototocol handler for [`Transaction`] messages
    /// and registers it to the provided P2P network, using the default session flag.
    pub async fn init(p2p: &P2pPtr) -> ProtocolTxHandlerPtr {
        debug!(
            target: "darkfid::proto::protocol_tx::init",
            "Adding ProtocolTx to the protocol registry"
        );

        let handler = ProtocolGenericHandler::new(p2p, "ProtocolTx", SESSION_DEFAULT).await;

        Arc::new(Self { handler })
    }

    /// Start the `ProtocolTx` background task.
    pub async fn start(
        &self,
        executor: &ExecutorPtr,
        validator: &ValidatorPtr,
        subscriber: JsonSubscriber,
    ) -> Result<()> {
        debug!(
            target: "darkfid::proto::protocol_tx::start",
            "Starting ProtocolTx handler task..."
        );

        self.handler.task.clone().start(
            handle_receive_tx(self.handler.clone(), validator.clone(), subscriber),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "darkfid::proto::protocol_tx::start", "Failed starting ProtocolTx handler task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        debug!(
            target: "darkfid::proto::protocol_tx::start",
            "ProtocolTx handler task started!"
        );

        Ok(())
    }

    /// Stop the `ProtocolTx` background task.
    pub async fn stop(&self) {
        debug!(target: "darkfid::proto::protocol_tx::stop", "Terminating ProtocolTx handler task...");
        self.handler.task.stop().await;
        debug!(target: "darkfid::proto::protocol_tx::stop", "ProtocolTx handler task terminated!");
    }
}

/// Background handler function for ProtocolTx.
async fn handle_receive_tx(
    handler: ProtocolGenericHandlerPtr<Transaction, Transaction>,
    validator: ValidatorPtr,
    subscriber: JsonSubscriber,
) -> Result<()> {
    debug!(target: "darkfid::proto::protocol_tx::handle_receive_tx", "START");
    loop {
        // Wait for a new transaction message
        let (channel, tx) = match handler.receiver.recv().await {
            Ok(r) => r,
            Err(e) => {
                debug!(
                    target: "darkfid::proto::protocol_tx::handle_receive_tx",
                    "recv fail: {e}"
                );
                continue
            }
        };

        // Check if node has finished syncing its blockchain
        let mut validator = validator.write().await;
        if !validator.synced {
            debug!(
                target: "darkfid::proto::protocol_tx::handle_receive_tx",
                "Node still syncing blockchain, skipping..."
            );
            handler.send_action(channel, ProtocolGenericAction::Skip).await;
            continue
        }

        // Append transaction
        if let Err(e) = validator.append_tx(&tx, true).await {
            debug!(
                target: "darkfid::proto::protocol_tx::handle_receive_tx",
                "append_tx fail: {e}"
            );
            handler.send_action(channel, ProtocolGenericAction::Skip).await;
            continue
        }

        // Signal handler to broadcast the valid transaction to rest nodes
        handler.send_action(channel, ProtocolGenericAction::Broadcast).await;

        // Notify subscriber
        let encoded_tx = JsonValue::String(base64::encode(&serialize_async(&tx).await));
        subscriber.notify(vec![encoded_tx].into()).await;
    }
}
