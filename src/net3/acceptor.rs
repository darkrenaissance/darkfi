use async_std::sync::Arc;

use log::error;
use smol::Executor;
use url::Url;

use crate::{
    system::{StoppableTask, StoppableTaskPtr, Subscriber, SubscriberPtr, Subscription},
    Error, Result,
};

use super::{Channel, ChannelPtr, TcpTransport, Transport, TransportListener, TransportName};

/// Atomic pointer to Acceptor class.
pub type AcceptorPtr = Arc<Acceptor>;

/// Create inbound socket connections.
pub struct Acceptor {
    channel_subscriber: SubscriberPtr<Result<ChannelPtr>>,
    task: StoppableTaskPtr,
}

impl Acceptor {
    /// Create new Acceptor object.
    pub fn new() -> Arc<Self> {
        Arc::new(Self { channel_subscriber: Subscriber::new(), task: StoppableTask::new() })
    }
    /// Start accepting inbound socket connections. Creates a listener to start
    /// listening on a local socket address. Then runs an accept loop in a new
    /// thread, erroring if a connection problem occurs.
    pub async fn start(
        self: Arc<Self>,
        accept_url: Url,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        let transport_name = TransportName::try_from(accept_url.clone())?;
        match transport_name {
            TransportName::Tcp(upgrade) => {
                let transport = TcpTransport::new(None, 1024);
                let listener = transport.listen_on(accept_url.clone());

                if let Err(err) = listener {
                    error!("Setup failed: {}", err);
                    return Err(Error::BindFailed(accept_url.clone().to_string()))
                }

                let listener = listener?.await;

                if let Err(err) = listener {
                    error!("Bind listener failed: {}", err);
                    return Err(Error::BindFailed(accept_url.to_string()))
                }

                let listener = listener?;

                match upgrade {
                    None => {
                        self.accept(Box::new(listener), executor);
                    }
                    Some(u) if u == "tls" => {
                        let tls_listener = transport.upgrade_listener(listener)?.await?;
                        self.accept(Box::new(tls_listener), executor);
                    }
                    // TODO hanle unsupported upgrade
                    Some(_) => todo!(),
                }
            }
            TransportName::Tor(_upgrade) => todo!(),
        }
        Ok(())
    }

    /// Stop accepting inbound socket connections.
    pub async fn stop(&self) {
        // Send stop signal
        self.task.stop().await;
    }

    /// Start receiving network messages.
    pub async fn subscribe(self: Arc<Self>) -> Subscription<Result<ChannelPtr>> {
        self.channel_subscriber.clone().subscribe().await
    }

    /// Run the accept loop in a new thread and error if a connection problem
    /// occurs.
    fn accept(self: Arc<Self>, listener: Box<dyn TransportListener>, executor: Arc<Executor<'_>>) {
        self.task.clone().start(
            self.clone().run_accept_loop(listener),
            |result| self.handle_stop(result),
            Error::ServiceStopped,
            executor,
        );
    }

    /// Run the accept loop.
    async fn run_accept_loop(self: Arc<Self>, listener: Box<dyn TransportListener>) -> Result<()> {
        while let Ok((stream, peer_addr)) = listener.next().await {
            let channel = Channel::new(stream, peer_addr).await;
            self.channel_subscriber.notify(Ok(channel)).await;
        }
        Ok(())
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
