use async_std::{stream::StreamExt, sync::Arc};
use std::net::SocketAddr;

use futures_rustls::TlsStream;
use log::error;
use smol::Executor;
use url::Url;

use crate::{
    system::{StoppableTask, StoppableTaskPtr, Subscriber, SubscriberPtr, Subscription},
    Error, Result,
};

use super::{Channel, ChannelPtr, TcpTransport, TlsTransport, Transport};

/// A helper function to convert peer addr to Url and add scheme
fn peer_addr_to_url(addr: SocketAddr, scheme: &str) -> Result<Url> {
    let url = Url::parse(&format!("{}://{}", scheme, addr.to_string()))?;
    Ok(url)
}

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
    pub async fn subscribe(self: Arc<Self>) -> Subscription<Result<ChannelPtr>> {
        self.channel_subscriber.clone().subscribe().await
    }

    /// Run the accept loop in a new thread and error if a connection problem
    /// occurs.
    fn accept(self: Arc<Self>, accept_addr: Url, executor: Arc<Executor<'_>>) {
        self.task.clone().start(
            self.clone().run_accept_loop(accept_addr),
            |result| self.handle_stop(result),
            Error::ServiceStopped,
            executor,
        );
    }

    /// Run the accept loop.
    async fn run_accept_loop(self: Arc<Self>, accept_url: Url) -> Result<()> {
        match accept_url.scheme() {
            "tcp" => {
                let transport = TcpTransport::new(None, 1024);
                let listener = transport.listen_on(accept_url);

                if let Err(err) = listener {
                    error!("Setup failed: {}", err);
                    return Err(Error::OperationFailed)
                }

                let listener = listener?.await;

                if let Err(err) = listener {
                    error!("Bind listener failed: {}", err);
                    return Err(Error::OperationFailed)
                }

                let listener = listener?;
                let mut incoming = listener.incoming();
                while let Some(stream) = incoming.next().await {
                    let result: Result<()> = {
                        let stream = stream?;
                        let peer_addr = peer_addr_to_url(stream.peer_addr()?, "tcp")?;
                        let channel = Channel::new(Box::new(stream), peer_addr).await;
                        self.channel_subscriber.notify(Ok(channel)).await;
                        Ok(())
                    };

                    if let Err(err) = result {
                        error!("Error listening for connections: {}", err);
                    }
                }
            }
            "tls" => {
                let transport = TlsTransport::new(None, 1024);

                let listener = transport.listen_on(accept_url);

                if let Err(err) = listener {
                    error!("Setup failed: {}", err);
                    return Err(Error::OperationFailed)
                }

                let listener = listener?.await;

                if let Err(err) = listener {
                    error!("Bind listener failed: {}", err);
                    return Err(Error::OperationFailed)
                }

                let (acceptor, listener) = listener?;

                let mut incoming = listener.incoming();
                while let Some(stream) = incoming.next().await {
                    let result: Result<()> = {
                        let stream = stream?;
                        let peer_addr = peer_addr_to_url(stream.peer_addr()?, "tls")?;

                        let stream = acceptor.accept(stream).await;

                        if let Err(err) = stream {
                            error!("Error wraping the connection with tls: {}", err);
                            return Err(Error::ServiceStopped)
                        }

                        let stream = stream?;
                        let channel =
                            Channel::new(Box::new(TlsStream::Server(stream)), peer_addr).await;
                        self.channel_subscriber.notify(Ok(channel)).await;
                        Ok(())
                    };

                    if let Err(err) = result {
                        error!("Error listening for connections: {}", err);
                    }
                }
            }
            "tor" => todo!(),
            _ => unimplemented!(),
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
