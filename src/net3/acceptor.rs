use async_std::{stream::StreamExt, sync::Arc};

use futures_rustls::TlsStream;
use smol::Executor;
use url::Url;

use crate::{
    system::{StoppableTask, StoppableTaskPtr, Subscriber, SubscriberPtr, Subscription},
    Error, Result,
};

use super::{Channel, ChannelPtr, TcpTransport, TlsTransport, Transport};

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
                let listener = transport.listen_on(accept_url)?.await?;
                let mut incoming = listener.incoming();
                while let Some(stream) = incoming.next().await {
                    let stream = stream?;
                    let mut peer_addr = Url::parse(&stream.peer_addr()?.to_string())?;
                    peer_addr.set_scheme("tcp")?;
                    let channel = Channel::new(Box::new(stream), peer_addr).await;
                    self.channel_subscriber.notify(Ok(channel)).await;
                }
            }
            "tls" => {
                let transport = TlsTransport::new(None, 1024);
                let (acceptor, listener) = transport.listen_on(accept_url)?.await?;
                let mut incoming = listener.incoming();
                while let Some(stream) = incoming.next().await {
                    let stream = stream?;
                    let mut peer_addr = Url::parse(&stream.peer_addr()?.to_string())?;
                    peer_addr.set_scheme("tls")?;
                    let stream = acceptor.accept(stream).await?;
                    let channel =
                        Channel::new(Box::new(TlsStream::Server(stream)), peer_addr).await;
                    self.channel_subscriber.notify(Ok(channel)).await;
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

    ///// Single attempt to accept an incoming connection. Stops after one
    ///// attempt.
    //async fn tick_accept(&self, listener: &TcpListener) -> Result<ChannelPtr> {
    //    let (stream, peer_addr) = match listener.accept().await {
    //        Ok((s, a)) => (s, a),
    //        Err(err) => {
    //            error!("Error listening for connections: {}", err);
    //            return Err(Error::ServiceStopped)
    //        }
    //    };
    //    info!("Accepted client: {}", peer_addr);

    //    let channel = Channel::new(Box::new(stream), peer_addr).await;
    //    Ok(channel)
    //}
}
