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
    Result,
};
use log::info;

/// `Foo` messages broadcast protocol
pub mod protocol_foo;
pub use protocol_foo::{ProtocolFooHandler, ProtocolFooHandlerPtr};

/// `Bar` messages broadcast protocol
pub mod protocol_bar;
pub use protocol_bar::{ProtocolBarHandler, ProtocolBarHandlerPtr};

/// Atomic pointer to the Denial-of-service Analysis Multitool P2P protocols handler.
pub type DamP2pHandlerPtr = Arc<DamP2pHandler>;

/// Denial-of-service Analysis Multitool P2P protocols handler.
pub struct DamP2pHandler {
    /// P2P network pointer
    pub p2p: P2pPtr,
    /// `ProtocolFoo` messages handler
    foo_handler: ProtocolFooHandlerPtr,
    /// `ProtocolBar` messages handler
    bar_handler: ProtocolBarHandlerPtr,
}

impl DamP2pHandler {
    /// Initialize a Denial-of-service Analysis Multitool P2P protocols handler.
    ///
    /// A new P2P instance is generated using provided settings and all
    /// corresponding protocols are registered.
    pub async fn init(settings: &Settings, executor: &ExecutorPtr) -> Result<DamP2pHandlerPtr> {
        info!(
            target: "damd::proto::mod::DamP2pHandler::init",
            "Initializing a new Denial-of-service Analysis Multitool P2P handler..."
        );

        // Generate a new P2P instance
        let p2p = P2p::new(settings.clone(), executor.clone()).await?;

        // Generate a new `ProtocolFoo` messages handler
        let foo_handler = ProtocolFooHandler::init(&p2p).await;

        // Generate a new `ProtocolBar` messages handler
        let bar_handler = ProtocolBarHandler::init(&p2p).await;

        info!(
            target: "damd::proto::mod::DamP2pHandler::init",
            "Denial-of-service Analysis Multitool P2P handler generated successfully!"
        );

        Ok(Arc::new(Self { p2p, foo_handler, bar_handler }))
    }

    /// Start the Denial-of-service Analysis Multitool P2P protocols handler.
    pub async fn start(
        &self,
        executor: &ExecutorPtr,
        subscribers: &HashMap<&'static str, JsonSubscriber>,
    ) -> Result<()> {
        info!(
            target: "damd::proto::mod::DamP2pHandler::start",
            "Starting the Denial-of-service Analysis Multitool P2P handler..."
        );

        // Start the `ProtocolFoo` messages handler
        let subscriber = subscribers.get("foo").unwrap().clone();
        self.foo_handler.start(executor, subscriber).await?;

        // Start the `ProtocolBar` messages handler
        let subscriber = subscribers.get("bar").unwrap().clone();
        self.bar_handler.start(executor, subscriber).await?;

        // Start the P2P instance
        self.p2p.clone().start().await?;

        info!(
            target: "damd::proto::mod::DamP2pHandler::start",
            "Denial-of-service Analysis Multitool P2P handler started successfully!"
        );

        Ok(())
    }

    /// Stop the Denial-of-service Analysis P2P protocols handler.
    pub async fn stop(&self) {
        info!(target: "damd::proto::mod::DamP2pHandler::stop", "Terminating Denial-of-service Analysis Multitool P2P handler...");

        // Stop the P2P instance
        self.p2p.stop().await;

        // Start the `ProtocolFoo` messages handler
        self.foo_handler.stop().await;

        // Start the `ProtocolBar` messages handler
        self.bar_handler.stop().await;

        info!(target: "damd::proto::mod::DamP2pHandler::stop", "Denial-of-service Analysis Multitool P2P handler terminated successfully!");
    }
}
