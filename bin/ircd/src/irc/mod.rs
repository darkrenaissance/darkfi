use async_std::{net::TcpListener, sync::Arc};
use std::{fs::File, net::SocketAddr};

use async_executor::Executor;
use futures::{io::BufReader, AsyncRead, AsyncReadExt, AsyncWrite};
use futures_rustls::{rustls, TlsAcceptor};
use fxhash::FxHashMap;
use log::{error, info};

use darkfi::{net::P2pPtr, system::SubscriberPtr, util::expand_path, Error, Result};

use crate::{
    buffers::{ArcPrivmsgsBuffer, SeenIds},
    settings::Args,
    ChannelInfo, ContactInfo, Privmsg,
};

mod client;

pub use client::IrcClient;

pub struct IrcServer {
    settings: Args,
    privmsgs_buffer: ArcPrivmsgsBuffer,
    seen_msg_ids: SeenIds,
    auto_channels: Vec<String>,
    password: String,
    configured_chans: FxHashMap<String, ChannelInfo>,
    configured_contacts: FxHashMap<String, ContactInfo>,
    p2p: P2pPtr,
    p2p_notifiers: SubscriberPtr<Privmsg>,
}

impl IrcServer {
    pub async fn new(
        settings: Args,
        privmsgs_buffer: ArcPrivmsgsBuffer,
        seen_msg_ids: SeenIds,
        auto_channels: Vec<String>,
        password: String,
        configured_chans: FxHashMap<String, ChannelInfo>,
        configured_contacts: FxHashMap<String, ContactInfo>,
        p2p: P2pPtr,
        p2p_notifiers: SubscriberPtr<Privmsg>,
    ) -> Result<Self> {
        Ok(Self {
            settings,
            privmsgs_buffer,
            seen_msg_ids,
            auto_channels,
            password,
            configured_chans,
            configured_contacts,
            p2p,
            p2p_notifiers,
        })
    }

    /// Start listening to new irc clients connecting to the irc server address
    /// then spawn new connections
    pub async fn start(&self, executor: Arc<Executor<'_>>) -> Result<()> {
        let (listener, acceptor) = self.setup_listener().await?;
        info!("[IRC SERVER] listening on {}", self.settings.irc_listen);

        loop {
            let (stream, peer_addr) = match listener.accept().await {
                Ok((s, a)) => (s, a),
                Err(e) => {
                    error!("[IRC SERVER] Failed accepting new connections: {}", e);
                    continue
                }
            };

            let result = if let Some(acceptor) = acceptor.clone() {
                let stream = match acceptor.accept(stream).await {
                    Ok(s) => s,
                    Err(e) => {
                        error!("[IRC SERVER] Failed accepting TLS connection: {}", e);
                        continue
                    }
                };
                self.process_connection(executor.clone(), stream, peer_addr).await
            } else {
                self.process_connection(executor.clone(), stream, peer_addr).await
            };

            if let Err(e) = result {
                error!("[IRC SERVER] Failed processing connection {}: {}", peer_addr, e);
                continue
            };

            info!("[IRC SERVER] Accept new connection: {}", peer_addr);
        }
    }

    /// On every new connection create new IrcClient which will process the messages
    async fn process_connection<C: AsyncRead + AsyncWrite + Send + Unpin + 'static>(
        &self,
        executor: Arc<Executor<'_>>,
        stream: C,
        peer_addr: SocketAddr,
    ) -> Result<()> {
        let (reader, writer) = stream.split();

        let reader = BufReader::new(reader);

        // New subscription
        let p2p_subscription = self.p2p_notifiers.clone().subscribe().await;

        // New irc connection
        let mut client = IrcClient::new(
            writer,
            peer_addr,
            self.privmsgs_buffer.clone(),
            self.seen_msg_ids.clone(),
            self.password.clone(),
            self.auto_channels.clone(),
            self.configured_chans.clone(),
            self.configured_contacts.clone(),
            self.p2p.clone(),
            self.p2p_notifiers.clone(),
            p2p_subscription,
        );

        executor
            .spawn(async move {
                client.listen(reader).await;
            })
            .detach();

        Ok(())
    }

    /// Setup a listener for irc server
    async fn setup_listener(&self) -> Result<(TcpListener, Option<TlsAcceptor>)> {
        let listenaddr = self.settings.irc_listen.socket_addrs(|| None)?[0];
        let listener = TcpListener::bind(listenaddr).await?;

        let acceptor = match self.settings.irc_listen.scheme() {
            "tls" => {
                // openssl genpkey -algorithm ED25519 > example.com.key
                // openssl req -new -out example.com.csr -key example.com.key
                // openssl x509 -req -days 700 -in example.com.csr -signkey example.com.key -out example.com.crt

                if self.settings.irc_tls_secret.is_none() || self.settings.irc_tls_cert.is_none() {
                    error!("[IRC SERVER] To listen using TLS, please set irc_tls_secret and irc_tls_cert in your config file.");
                    return Err(Error::KeypairPathNotFound)
                }

                let file =
                    File::open(expand_path(self.settings.irc_tls_secret.as_ref().unwrap())?)?;
                let mut reader = std::io::BufReader::new(file);
                let secret = &rustls_pemfile::pkcs8_private_keys(&mut reader)?[0];
                let secret = rustls::PrivateKey(secret.clone());

                let file = File::open(expand_path(self.settings.irc_tls_cert.as_ref().unwrap())?)?;
                let mut reader = std::io::BufReader::new(file);
                let certificate = &rustls_pemfile::certs(&mut reader)?[0];
                let certificate = rustls::Certificate(certificate.clone());

                let config = rustls::ServerConfig::builder()
                    .with_safe_defaults()
                    .with_no_client_auth()
                    .with_single_cert(vec![certificate], secret)?;

                let acceptor = TlsAcceptor::from(Arc::new(config));
                Some(acceptor)
            }
            _ => None,
        };
        Ok((listener, acceptor))
    }
}
