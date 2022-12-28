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

use std::{collections::HashMap, fs::File, net::SocketAddr};

use async_std::{
    net::TcpListener,
    sync::{Arc, Mutex},
};
use futures::{io::BufReader, AsyncRead, AsyncReadExt, AsyncWrite};
use futures_rustls::{rustls, TlsAcceptor};
use log::{error, info};
use smol::Executor;

use darkfi::{
    net::P2pPtr,
    system::SubscriberPtr,
    util::path::{expand_path, get_config_path},
    Error, Result,
};

use crate::{
    buffers::SeenIds,
    settings::{
        parse_configured_channels, parse_configured_contacts, Args, ChannelInfo, ContactInfo,
        CONFIG_FILE,
    },
    Privmsg,
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
    pub capabilities: HashMap<String, bool>,

    // channels and contacts
    pub auto_channels: Vec<String>,
    pub configured_chans: HashMap<String, ChannelInfo>,
    pub configured_contacts: HashMap<String, ContactInfo>,
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

        let mut capabilities = HashMap::new();
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
    seen: Arc<Mutex<SeenIds>>,
    p2p: P2pPtr,
    notify_clients: SubscriberPtr<Privmsg>,
}

impl IrcServer {
    pub async fn new(
        settings: Args,
        seen: Arc<Mutex<SeenIds>>,
        p2p: P2pPtr,
        notify_clients: SubscriberPtr<Privmsg>,
    ) -> Result<Self> {
        let irc_config = IrcConfig::new(&settings)?;
        Ok(Self { settings, irc_config, seen, p2p, notify_clients })
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
        let client_subscription = self.notify_clients.clone().subscribe().await;

        // New irc connection
        let mut client = IrcClient::new(
            writer,
            peer_addr,
            self.seen.clone(),
            self.irc_config.clone(),
            self.p2p.clone(),
            self.notify_clients.clone(),
            client_subscription,
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
