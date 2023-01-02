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

use async_std::sync::Mutex;
use std::sync::Arc;

use futures::Future;
use log::*;
use smol::Task;

use crate::{system::ExecutorPtr, Result};

use super::super::ChannelPtr;

/// Pointer to protocol jobs manager.
pub type ProtocolJobsManagerPtr = Arc<ProtocolJobsManager>;

/// Manages the tasks for the network protocol. Used by other connection
/// protocols to handle asynchronous task execution across the network. Runs all
/// tasks that are handed to it on an executor that has stopping functionality.
pub struct ProtocolJobsManager {
    name: &'static str,
    channel: ChannelPtr,
    tasks: Mutex<Vec<Task<Result<()>>>>,
}

impl ProtocolJobsManager {
    /// Create a new protocol jobs manager.
    pub fn new(name: &'static str, channel: ChannelPtr) -> Arc<Self> {
        Arc::new(Self { name, channel, tasks: Mutex::new(Vec::new()) })
    }

    /// Runs the task on an executor. Prepares to stop all tasks when the
    /// channel is closed.
    pub fn start(self: Arc<Self>, executor: ExecutorPtr<'_>) {
        executor.spawn(self.handle_stop()).detach()
    }

    /// Spawns a new task and adds it to the internal queue.
    pub async fn spawn<'a, F>(&self, future: F, executor: ExecutorPtr<'a>)
    where
        F: Future<Output = Result<()>> + Send + 'a,
    {
        self.tasks.lock().await.push(executor.spawn(future))
    }

    /// Waits for a stop signal, then closes all tasks. Insures that all tasks
    /// are stopped when a channel closes. Called in start().
    async fn handle_stop(self: Arc<Self>) {
        let stop_sub = self.channel.clone().subscribe_stop().await;

        if stop_sub.is_ok() {
            // Wait for the stop signal
            stop_sub.unwrap().receive().await;
        }

        self.close_all_tasks().await
    }

    /// Closes all open tasks. Takes all the tasks from the internal queue and
    /// closes them.
    async fn close_all_tasks(self: Arc<Self>) {
        debug!(target: "net::protocol_jobs_manager",
            "ProtocolJobsManager::close_all_tasks() [START, name={}, addr={}]",
            self.name,
            self.channel.address()
        );
        // Take all the tasks from our internal queue...
        let tasks = std::mem::take(&mut *self.tasks.lock().await);
        for task in tasks {
            // ... and cancel them
            let _ = task.cancel().await;
        }
    }
}
