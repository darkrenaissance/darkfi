use async_std::sync::Mutex;
use futures::Future;
use log::*;
use smol::Task;
use std::sync::Arc;

use crate::net::error::NetResult;
use crate::net::ChannelPtr;
use crate::system::ExecutorPtr;

/// Pointer to protocol jobs manager.
pub type ProtocolJobsManagerPtr = Arc<ProtocolJobsManager>;

/// Manages the tasks for the network protocol.
pub struct ProtocolJobsManager {
    name: &'static str,
    channel: ChannelPtr,
    tasks: Mutex<Vec<Task<NetResult<()>>>>,
}

impl ProtocolJobsManager {
    /// Create a new protocol jobs manager.
    pub fn new(name: &'static str, channel: ChannelPtr) -> Arc<Self> {
        Arc::new(Self {
            name,
            channel,
            tasks: Mutex::new(Vec::new()),
        })
    }

    /// Runs the task on an executor. Prepares to stop all tasks when the channel is closed.
    pub fn start(self: Arc<Self>, executor: ExecutorPtr<'_>) {
        executor.spawn(self.handle_stop()).detach()
    }

    /// Spawns a new task and adds it to the internal queue.
    pub async fn spawn<'a, F>(&self, future: F, executor: ExecutorPtr<'a>)
    where
        F: Future<Output = NetResult<()>> + Send + 'a,
    {
        self.tasks.lock().await.push(executor.spawn(future))
    }

    /// Waits for a stop signal, then closes all tasks. Insures that all tasks are stopped when a
    /// channel closes. Called in start().
    async fn handle_stop(self: Arc<Self>) {
        let stop_sub = self.channel.clone().subscribe_stop().await;

        // Wait for the stop signal
        // Not interested in the exact error
        let _ = stop_sub.receive().await;

        self.close_all_tasks().await
    }

    /// Closes all open tasks. Takes all the tasks from the internal queue and closes them.
    async fn close_all_tasks(self: Arc<Self>) {
        debug!(target: "net",
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
