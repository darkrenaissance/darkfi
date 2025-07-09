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

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use darkfi::{
    net::{channel::ChannelPtr, P2pPtr},
    rpc::jsonrpc::JsonSubscriber,
    system::{ExecutorPtr, StoppableTask, StoppableTaskPtr},
    Error, Result,
};
use smol::lock::Mutex;
use tinyjson::JsonValue;
use tracing::{debug, error, info};

use crate::proto::{
    protocol_bar::Bar,
    protocol_foo::{FooRequest, FooResponse},
};

/// Atomic pointer to the Denial-of-service Analysis Multitool flooder.
pub type DamFlooderPtr = Arc<DamFlooder>;

/// Denial-of-service Analysis Multitool flooder.
pub struct DamFlooder {
    /// P2P network pointer
    p2p: P2pPtr,
    /// Executor to spawn flooding tasks
    executor: ExecutorPtr,
    /// Set to keep track of all the spawned tasks
    tasks: Arc<Mutex<HashSet<StoppableTaskPtr>>>,
}

impl DamFlooder {
    /// Initialize a Denial-of-service Analysis Multitool flooder.
    pub fn init(p2p: &P2pPtr, ex: &ExecutorPtr) -> DamFlooderPtr {
        Arc::new(Self {
            p2p: p2p.clone(),
            executor: ex.clone(),
            tasks: Arc::new(Mutex::new(HashSet::new())),
        })
    }

    /// Start the Denial-of-service Analysis Multitool flooder.
    pub async fn start(&self, subscribers: &HashMap<&'static str, JsonSubscriber>, limit: u32) {
        info!(
            target: "damd::flooder::DamFlooder::start",
            "Starting the Denial-of-service Analysis Multitool flooder..."
        );

        // Check if tasks already exist
        let mut lock = self.tasks.lock().await;
        if !lock.is_empty() {
            info!(
                target: "damd::flooder::DamFlooder::start",
                "Denial-of-service Analysis Multitool flooder already started!"
            );
            return
        }

        // Spawn a task for each connected peer for `Foo` messages, since we expect responses
        for peer in self.p2p.hosts().channels() {
            let task = StoppableTask::new();
            task.clone().start(
                flood_foo(self.p2p.settings().read().await.outbound_connect_timeout, peer, subscribers.get("attack_foo").unwrap().clone(), limit),
                |res| async move {
                    match res {
                        Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                        Err(e) => error!(target: "damd::Damd::start", "Failed starting flood foo task: {e}")
                    }
                },
                Error::DetachedTaskStopped,
                self.executor.clone(),
            );
            lock.insert(task);
        }

        // Spawn a task for `Bar` messages to broadcast to everyone
        let task = StoppableTask::new();
        task.clone().start(
            flood_bar(self.p2p.clone(), subscribers.get("attack_bar").unwrap().clone(), limit),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => {
                        error!(target: "damd::Damd::start", "Failed starting flood bar task: {e}")
                    }
                }
            },
            Error::DetachedTaskStopped,
            self.executor.clone(),
        );
        lock.insert(task);

        info!(
            target: "damd::flooder::DamFlooder::start",
            "Denial-of-service Analysis Multitool flooder started successfully!"
        );
    }

    /// Stop the Denial-of-service Analysis flooder.
    pub async fn stop(&self) {
        info!(target: "damd::flooder::DamFlooder::stop", "Terminating Denial-of-service Analysis Multitool flooder...");

        // Check if tasks already terminated
        let mut lock = self.tasks.lock().await;
        if lock.is_empty() {
            info!(
                target: "damd::flooder::DamFlooder::start",
                "Denial-of-service Analysis Multitool flooder already terminated!"
            );
            return
        }

        // Terminate the tasks
        for task in lock.iter() {
            task.stop().await;
        }

        // Clean the set
        *lock = HashSet::new();
        info!(target: "damd::flooder::DamFlooder::stop", "Denial-of-service Analysis Multitool flooder terminated successfully!");
    }
}

/// Background flooder function for `ProtocolFoo`.
async fn flood_foo(
    comms_timeout: u64,
    peer: ChannelPtr,
    subscriber: JsonSubscriber,
    limit: u32,
) -> Result<()> {
    debug!(target: "damd::flooder::flood_foo", "START");
    // Communication setup
    let Ok(response_sub) = peer.subscribe_msg::<FooResponse>().await else {
        let notification =
            format!("Failure during `FooResponse` communication setup with peer: {peer:?}");
        error!(target: "damd::flooder::flood_foo", "{notification}");
        subscriber.notify(vec![JsonValue::String(notification)].into()).await;
        return Ok(())
    };

    // Flood the peer
    let mut message_index = 0;
    loop {
        // Node creates a `FooRequest` and sends it
        let message = format!("Flood message {message_index}");
        let notification = format!("Sending foo request to {peer:?}: {message}");
        info!(target: "damd::flooder::flood_foo", "{notification}");
        subscriber.notify(vec![JsonValue::String(notification)].into()).await;
        if let Err(e) = peer.send(&FooRequest { message }).await {
            let notification = format!("Failure during `FooRequest` send to peer {peer:?}: {e}");
            error!(target: "damd::flooder::flood_foo", "{notification}");
            subscriber.notify(vec![JsonValue::String(notification)].into()).await;
            return Ok(())
        };

        // Node waits for response
        let Ok(response) = response_sub.receive_with_timeout(comms_timeout).await else {
            let notification =
                format!("Timeout while waiting for `FooResponse` from peer: {peer:?}");
            error!(target: "damd::flooder::flood_foo", "{notification}");
            subscriber.notify(vec![JsonValue::String(notification)].into()).await;
            return Ok(())
        };

        // Notify subscriber
        let notification = format!("Retrieved foo response from {peer:?}: {}", response.code);
        info!(target: "damd::flooder::flood_foo", "{notification}");
        subscriber.notify(vec![JsonValue::String(notification)].into()).await;
        message_index += 1;

        // Check limit
        if limit != 0 && message_index > limit {
            debug!(target: "damd::flooder::flood_foo", "STOP");
            info!(target: "damd::flooder::flood_foo", "Flood limit reached!");
            return Ok(())
        }
    }
}

/// Background flooder function for `ProtocolBar`.
async fn flood_bar(p2p: P2pPtr, subscriber: JsonSubscriber, limit: u32) -> Result<()> {
    debug!(target: "damd::flooder::flood_bar", "START");

    // Flood the network, if we are connected to peers
    let mut message_index = 0;
    while p2p.is_connected() {
        // Node creates a `Bar` message and broadcasts it
        let message = format!("Flood message {message_index}");
        let notification = format!("Broadcasting bar message: {message}");
        info!(target: "damd::flooder::flood_bar", "{notification}");
        subscriber.notify(vec![JsonValue::String(notification)].into()).await;
        p2p.broadcast(&Bar { message }).await;
        message_index += 1;

        // Check limit
        if limit != 0 && message_index > limit {
            debug!(target: "damd::flooder::flood_bar", "STOP");
            info!(target: "damd::flooder::flood_foo", "Flood limit reached!");
            return Ok(())
        }
    }

    debug!(target: "damd::flooder::flood_bar", "STOP");
    subscriber
        .notify(vec![JsonValue::String(String::from("We are not connected to any peers"))].into())
        .await;
    Ok(())
}
