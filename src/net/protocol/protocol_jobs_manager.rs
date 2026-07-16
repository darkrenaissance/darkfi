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

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use smol::{future::Future, lock::Mutex, Executor, Task};
use tracing::{debug, trace};

use super::super::channel::ChannelPtr;
use crate::Result;

/// Pointer to protocol jobs manager
pub type ProtocolJobsManagerPtr = Arc<ProtocolJobsManager>;

pub struct ProtocolJobsManager {
    name: &'static str,
    channel: ChannelPtr,
    tasks: Mutex<Vec<Task<Result<()>>>>,
    stopped: AtomicBool,
}

impl ProtocolJobsManager {
    /// Create a new protocol jobs manager
    pub fn new(name: &'static str, channel: ChannelPtr) -> ProtocolJobsManagerPtr {
        Arc::new(Self { name, channel, tasks: Mutex::new(vec![]), stopped: AtomicBool::new(false) })
    }

    /// Returns configured name
    pub fn name(self: Arc<Self>) -> &'static str {
        self.name
    }

    /// Runs the task on an executor
    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        let channel = self.channel.clone();
        channel.add_cleanup_task(executor.spawn(self.handle_stop())).await
    }

    /// Spawns a new task and adds it to the internal queue
    pub async fn spawn<'a, F>(&self, future: F, executor: Arc<Executor<'a>>)
    where
        F: Future<Output = Result<()>> + Send + 'a,
    {
        let task = executor.spawn(future);
        let mut tasks = self.tasks.lock().await;
        if self.stopped.load(Ordering::SeqCst) || self.channel.is_stopped() {
            drop(tasks);
            let _ = task.cancel().await;
            return
        }
        tasks.push(task)
    }

    /// Waits for a stop signal, then closes all tasks.
    /// Ensures that all tasks are stopped when a channel closes.
    /// Called in `start()`
    async fn handle_stop(self: Arc<Self>) {
        let stop_sub = self.channel.subscribe_stop().await;

        if let Ok(stop_sub) = stop_sub {
            // Wait for the stop signal
            stop_sub.receive().await;
        }

        self.stopped.store(true, Ordering::SeqCst);
        self.close_all_tasks().await
    }

    /// Closes all open tasks. Takes all the tasks from the internal queue.
    async fn close_all_tasks(&self) {
        debug!(
            target: "net::protocol_jobs_manager",
            "ProtocolJobsManager::close_all_tasks() [START, name={}, addr={}]",
            self.name, self.channel.display_address(),
        );

        let tasks = std::mem::take(&mut *self.tasks.lock().await);

        trace!(target: "net::protocol_jobs_manager", "Cancelling {} tasks", tasks.len());
        let mut i = 0;
        #[allow(clippy::explicit_counter_loop)]
        for task in tasks {
            trace!(target: "net::protocol_jobs_manager", "Cancelling task #{i}");
            let _ = task.cancel().await;
            trace!(target: "net::protocol_jobs_manager", "Cancelled task #{i}");
            i += 1;
        }
    }
}
