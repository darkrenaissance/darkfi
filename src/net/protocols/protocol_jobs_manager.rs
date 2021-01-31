use async_std::sync::Mutex;
use futures::Future;
use log::*;
use smol::Task;
use std::sync::Arc;

use crate::net::error::NetResult;
use crate::net::ChannelPtr;
use crate::system::ExecutorPtr;

pub type ProtocolJobsManagerPtr = Arc<ProtocolJobsManager>;

pub struct ProtocolJobsManager {
    name: &'static str,
    channel: ChannelPtr,
    tasks: Mutex<Vec<Task<NetResult<()>>>>,
}

impl ProtocolJobsManager {
    pub fn new(name: &'static str, channel: ChannelPtr) -> Arc<Self> {
        Arc::new(Self {
            name,
            channel,
            tasks: Mutex::new(Vec::new()),
        })
    }

    pub fn start(self: Arc<Self>, executor: ExecutorPtr<'_>) {
        executor.spawn(self.handle_stop()).detach()
    }

    pub async fn spawn<'a, F>(&self, future: F, executor: ExecutorPtr<'a>)
    where
        F: Future<Output = NetResult<()>> + Send + 'a,
    {
        self.tasks.lock().await.push(executor.spawn(future))
    }

    async fn handle_stop(self: Arc<Self>) {
        let stop_sub = self.channel.clone().subscribe_stop().await;

        // Wait for the stop signal
        // Not interested in the exact error
        let _ = stop_sub.receive().await;

        self.close_all_tasks().await
    }

    async fn close_all_tasks(self: Arc<Self>) {
        debug!(target: "net",
            "ProtocolJobsManager::close_all_tasks() [START, name={}, addr={}]",
            self.name,
            self.channel.address()
        );
        let tasks = std::mem::take(&mut *self.tasks.lock().await);
        for task in tasks {
            let _ = task.cancel().await;
        }
    }
}
