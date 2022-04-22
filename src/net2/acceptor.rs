use std::sync::Arc;

use smol::Executor;
use url::Url;

use crate::{
    system::{StoppableTask, StoppableTaskPtr, Subscriber, SubscriberPtr, Subscription},
    Error, Result,
};

use super::{Channel, ChannelPtr, Transport};

/// Atomic pointer to Acceptor class.
pub type AcceptorPtr<T> = Arc<Acceptor<T>>;

/// Create inbound socket connections.
pub struct Acceptor<T: Transport> {
    channel_subscriber: SubscriberPtr<Result<ChannelPtr<T>>>,
    task: StoppableTaskPtr,
}

impl<T: Transport> Acceptor<T> {
    /// Create new Acceptor object.
    pub fn new() -> Arc<Self> {
        Arc::new(Self { channel_subscriber: Subscriber::new(), task: StoppableTask::new() })
    }
    /// Start accepting inbound socket connections. Creates a listener to start
    /// listening on a local socket address. Then runs an accept loop in a new
    /// thread, erroring if a connection problem occurs.
    pub async fn start(
        self: Arc<Self>,
        accept_addr: Url,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        self.accept(accept_addr, executor);
        Ok(())
    }

    /// Stop accepting inbound socket connections.
    pub async fn stop(&self) {
        // Send stop signal
        self.task.stop().await;
    }

    /// Start receiving network messages.
    pub async fn subscribe(self: Arc<Self>) -> Subscription<Result<ChannelPtr<T>>> {
        self.channel_subscriber.clone().subscribe().await
    }

    /// Run the accept loop in a new thread and error if a connection problem
    /// occurs.
    fn accept(self: Arc<Self>, url: Url, executor: Arc<Executor<'_>>) {
        self.task.clone().start(
            self.clone().run_accept_loop(url),
            |result| self.handle_stop(result),
            Error::ServiceStopped,
            executor,
        );
    }

    /// Run the accept loop.
    async fn run_accept_loop(self: Arc<Self>, url: url::Url) -> Result<()> {
        let transport = T::new(None, 1024);
        let listener = Arc::new(transport.listen_on(url.clone())?.await?);
        loop {
            let stream = T::accept(listener.clone()).await?;
            let channel = Channel::<T>::new(stream, url.clone()).await;
            self.channel_subscriber.notify(Ok(channel)).await;
        }
    }

    /// Handles network errors. Panics if error passes silently, otherwise
    /// broadcasts the error.
    async fn handle_stop(self: Arc<Self>, result: Result<()>) {
        match result {
            Ok(()) => panic!("Acceptor task should never complete without error status"),
            Err(err) => {
                // Send this error to all channel subscribers
                let result = Err(err);
                self.channel_subscriber.notify(result).await;
            }
        }
    }
}
