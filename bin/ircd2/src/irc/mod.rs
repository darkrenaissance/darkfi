use async_std::{net::TcpListener, sync::Arc};
use std::{fs::File, net::SocketAddr};

use async_executor::Executor;
use futures::{io::BufReader, AsyncRead, AsyncReadExt, AsyncWrite};
use futures_rustls::{rustls, TlsAcceptor};
use fxhash::FxHashMap;
use log::{error, info};

use darkfi::{
    system::SubscriberPtr,
    util::path::{expand_path, get_config_path},
    Error, Result,
};

use crate::{
    privmsg::PrivMsgEvent,
    settings::{
        parse_configured_channels, parse_configured_contacts, Args, ChannelInfo, ContactInfo,
        CONFIG_FILE,
    },
};

mod client;

pub use client::IrcClient;

#[derive(Clone)]
pub struct IrcConfig {
    // init bool
    pub is_nick_init: bool,
    pub is_user_init: bool,
    pub is_registered: bool,
    pub is_cap_end: bool,
    pub is_pass_init: bool,

    // user config
    pub nickname: String,
    pub password: String,
    pub capabilities: FxHashMap<String, bool>,

    // channels and contacts
    pub auto_channels: Vec<String>,
    pub configured_chans: FxHashMap<String, ChannelInfo>,
    pub configured_contacts: FxHashMap<String, ContactInfo>,
}

impl IrcConfig {
    pub fn new(settings: &Args) -> Result<Self> {
        let password = settings.password.as_ref().unwrap_or(&String::new()).clone();

        let auto_channels = settings.autojoin.clone();

        // Pick up channel settings from the TOML configuration
        let cfg_path = get_config_path(settings.config.clone(), CONFIG_FILE)?;
        let toml_contents = std::fs::read_to_string(cfg_path)?;
        let configured_chans = parse_configured_channels(&toml_contents)?;
        let configured_contacts = parse_configured_contacts(&toml_contents)?;

        let mut capabilities = FxHashMap::default();
        capabilities.insert("no-history".to_string(), false);
        Ok(Self {
            is_nick_init: false,
            is_user_init: false,
            is_registered: false,
            is_cap_end: true,
            is_pass_init: false,
            nickname: "anon".to_string(),
            password,
            auto_channels,
            configured_chans,
            configured_contacts,
            capabilities,
        })
    }
}

pub struct IrcServer {
    settings: Args,
    irc_config: IrcConfig,
    clients_subscriptions: SubscriberPtr<PrivMsgEvent>,
}

impl IrcServer {
    pub async fn new(
        settings: Args,
        clients_subscriptions: SubscriberPtr<PrivMsgEvent>,
    ) -> Result<Self> {
        let irc_config = IrcConfig::new(&settings)?;
        Ok(Self { settings, irc_config, clients_subscriptions })
    }

    pub async fn start(&self, executor: Arc<Executor<'_>>) -> Result<()> {
        let (notifier, recv) = async_channel::unbounded();

        // Listen to msgs from clients
        executor.spawn(Self::listen_to_msgs(recv, self.clients_subscriptions.clone())).detach();

        // Start listening for new connections
        self.listen(notifier, executor.clone()).await?;

        Ok(())
    }

    /// Start listening to msgs from irc clients
    pub async fn listen_to_msgs(
        recv: async_channel::Receiver<(PrivMsgEvent, u64)>,
        clients_subscriptions: SubscriberPtr<PrivMsgEvent>,
    ) -> Result<()> {
        loop {
            let (msg, subscription_id) = recv.recv().await?;

            // TODO Add to View to prevent duplicate msg, since a client may has already added the
            // msg to its buffer

            // Since this will be added to the View directly, other clients connected to irc
            // server must get informed about this new msg
            clients_subscriptions.notify_with_exclude(msg, &[subscription_id]).await;

            // TODO broadcast to the p2p network
        }
    }

    /// Start listening to new connections from irc clients
    pub async fn listen(
        &self,
        notifier: async_channel::Sender<(PrivMsgEvent, u64)>,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
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
                // TLS connection
                let stream = match acceptor.accept(stream).await {
                    Ok(s) => s,
                    Err(e) => {
                        error!("[IRC SERVER] Failed accepting TLS connection: {}", e);
                        continue
                    }
                };
                self.process_connection(stream, peer_addr, notifier.clone(), executor.clone()).await
            } else {
                // TCP connection
                self.process_connection(stream, peer_addr, notifier.clone(), executor.clone()).await
            };

            if let Err(e) = result {
                error!("[IRC SERVER] Failed processing connection {}: {}", peer_addr, e);
                continue
            };

            info!("[IRC SERVER] Accept new connection: {}", peer_addr);
        }
    }

    /// On every new connection create new IrcClient
    async fn process_connection<C: AsyncRead + AsyncWrite + Send + Unpin + 'static>(
        &self,
        stream: C,
        peer_addr: SocketAddr,
        notifier: async_channel::Sender<(PrivMsgEvent, u64)>,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        let (reader, writer) = stream.split();
        let reader = BufReader::new(reader);

        // Subscription for the new client
        let client_subscription = self.clients_subscriptions.clone().subscribe().await;

        // New irc client
        let mut client = IrcClient::new(
            writer,
            reader,
            peer_addr,
            self.irc_config.clone(),
            notifier,
            client_subscription,
        );

        // Start listening and detach
        executor
            .spawn(async move {
                client.listen().await;
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
