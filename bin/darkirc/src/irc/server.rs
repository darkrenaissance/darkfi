/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use std::{collections::HashMap, fs::File, io::BufReader, path::PathBuf, sync::Arc};

use darkfi::{
    event_graph::Event,
    system::{StoppableTask, StoppableTaskPtr, Subscription},
    util::path::expand_path,
    Error, Result,
};
use futures_rustls::{
    rustls::{self, pki_types::PrivateKeyDer},
    TlsAcceptor,
};
use log::{debug, error, info};
use smol::{
    fs,
    lock::{Mutex, RwLock},
    net::{SocketAddr, TcpListener},
    prelude::{AsyncRead, AsyncWrite},
    Executor,
};
use url::Url;

use super::{client::Client, IrcChannel, IrcContact, Priv, Privmsg};
use crate::{
    crypto::saltbox,
    settings::{parse_autojoin_channels, parse_configured_channels, parse_configured_contacts},
    DarkIrc,
};

/// Max channel/nick length
pub const MAX_NICK_LEN: usize = 24;

/// Max message length
pub const MAX_MSG_LEN: usize = 512;

/// IRC server instance
pub struct IrcServer {
    /// DarkIrc instance
    pub darkirc: Arc<DarkIrc>,
    /// Path to the darkirc config file
    config_path: PathBuf,
    /// TCP listener
    listener: TcpListener,
    /// TLS acceptor
    acceptor: Option<TlsAcceptor>,
    /// Configured autojoin channels
    pub autojoin: RwLock<Vec<String>>,
    /// Configured IRC channels
    pub channels: RwLock<HashMap<String, IrcChannel>>,
    /// Configured IRC contacts
    pub contacts: RwLock<HashMap<String, IrcContact>>,
    /// Active client connections
    clients: Mutex<HashMap<u16, StoppableTaskPtr>>,
    /// IRC server Password
    pub password: String,
}

impl IrcServer {
    /// Instantiate a new IRC server. This function will try to bind a TCP socket,
    /// and optionally load a TLS certificate and key. To start the listening loop,
    /// call `IrcServer::listen()`.
    pub async fn new(
        darkirc: Arc<DarkIrc>,
        listen: Url,
        tls_cert: Option<String>,
        tls_secret: Option<String>,
        config_path: PathBuf,
        password: String,
    ) -> Result<Arc<Self>> {
        let scheme = listen.scheme();
        if scheme != "tcp" && scheme != "tcp+tls" {
            error!("IRC server supports listening only on tcp:// or tcp+tls://");
            return Err(Error::BindFailed(listen.to_string()))
        }

        if scheme == "tcp+tls" && (tls_cert.is_none() || tls_secret.is_none()) {
            error!("You must provide a TLS certificate and key if you want a TLS server");
            return Err(Error::BindFailed(listen.to_string()))
        }

        // Bind listener
        let listen_addr = listen.socket_addrs(|| None)?[0];
        let listener = TcpListener::bind(listen_addr).await?;
        let acceptor = match scheme {
            "tcp+tls" => {
                // openssl genpkey -algorithm ED25519 > example.com.key
                // openssl req -new -out example.com.csr -key example.com.key
                // openssl x509 -req -in example.com.csr -signkey example.com.key -out example.com.crt
                let f = File::open(expand_path(tls_secret.as_ref().unwrap())?)?;
                let mut reader = BufReader::new(f);
                let secret = PrivateKeyDer::Pkcs8(
                    rustls_pemfile::pkcs8_private_keys(&mut reader).next().unwrap().unwrap(),
                );

                let f = File::open(expand_path(tls_cert.as_ref().unwrap())?)?;
                let mut reader = BufReader::new(f);
                let cert = rustls_pemfile::certs(&mut reader).next().unwrap().unwrap();

                let config = rustls::ServerConfig::builder()
                    .with_no_client_auth()
                    .with_single_cert(vec![cert], secret)
                    .unwrap();

                let acceptor = TlsAcceptor::from(Arc::new(config));
                Some(acceptor)
            }
            _ => None,
        };

        let self_ = Arc::new(Self {
            darkirc,
            config_path,
            listener,
            acceptor,
            autojoin: RwLock::new(Vec::new()),
            channels: RwLock::new(HashMap::new()),
            contacts: RwLock::new(HashMap::new()),
            clients: Mutex::new(HashMap::new()),
            password,
        });

        // Load any channel/contact configuration.
        self_.rehash().await?;

        Ok(self_)
    }

    /// Reload the darkirc configuration file and reconfigure channels and contacts.
    pub async fn rehash(&self) -> Result<()> {
        let contents = fs::read_to_string(&self.config_path).await?;
        let contents = match toml::from_str(&contents) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed parsing TOML config: {}", e);
                return Err(Error::ParseFailed("Failed parsing TOML config"))
            }
        };

        // Parse autojoin channels
        let autojoin = parse_autojoin_channels(&contents)?;

        // Parse configured channels
        let channels = parse_configured_channels(&contents)?;

        // Parse configured contacts
        let contacts = parse_configured_contacts(&contents)?;

        // FIXME: This will remove clients' joined channels. They need to stay.
        // Only if everything is fine, replace.
        *self.autojoin.write().await = autojoin;
        *self.channels.write().await = channels;
        *self.contacts.write().await = contacts;

        Ok(())
    }

    /// Start accepting new IRC connections.
    pub async fn listen(self: Arc<Self>, ex: Arc<Executor<'_>>) -> Result<()> {
        loop {
            let (stream, peer_addr) = match self.listener.accept().await {
                Ok((s, a)) => (s, a),

                // As per usual accept(2) recommendations
                Err(e) if e.raw_os_error().is_some() => match e.raw_os_error().unwrap() {
                    libc::EAGAIN | libc::ECONNABORTED | libc::EPROTO | libc::EINTR => continue,
                    _ => {
                        error!("[IRC SERVER] Failed accepting connection: {}", e);
                        return Err(e.into())
                    }
                },

                Err(e) => {
                    error!("[IRC SERVER] Failed accepting new connection: {}", e);
                    continue
                }
            };

            match &self.acceptor {
                // Expecting encrypted TLS connection
                Some(acceptor) => {
                    let stream = match acceptor.accept(stream).await {
                        Ok(s) => s,
                        Err(e) => {
                            error!("[IRC SERVER] Failed accepting new TLS connection: {}", e);
                            continue
                        }
                    };

                    // Subscribe to incoming events and set up the connection.
                    let incoming = self.darkirc.event_graph.event_pub.clone().subscribe().await;
                    if let Err(e) = self
                        .clone()
                        .process_connection(stream, peer_addr, incoming, ex.clone())
                        .await
                    {
                        error!("[IRC SERVER] Failed processing new connection: {}", e);
                        continue
                    };
                }

                // Expecting plain TCP connection
                None => {
                    // Subscribe to incoming events and set up the connection.
                    let incoming = self.darkirc.event_graph.event_pub.clone().subscribe().await;
                    if let Err(e) = self
                        .clone()
                        .process_connection(stream, peer_addr, incoming, ex.clone())
                        .await
                    {
                        error!("[IRC SERVER] Failed processing new connection: {}", e);
                        continue
                    };
                }
            }

            info!("[IRC SERVER] Accepted new client connection at: {}", peer_addr);
        }
    }

    /// IRC client connection process.
    /// Sets up multiplexing between the server and client.
    /// Detaches the connection as a `StoppableTask`.
    async fn process_connection<C: AsyncRead + AsyncWrite + Send + Unpin + 'static>(
        self: Arc<Self>,
        stream: C,
        peer_addr: SocketAddr,
        incoming: Subscription<Event>,
        ex: Arc<Executor<'_>>,
    ) -> Result<()> {
        let port = peer_addr.port();
        let client = Client::new(self.clone(), incoming, peer_addr).await?;

        let conn_task = StoppableTask::new();
        self.clients.lock().await.insert(port, conn_task.clone());

        conn_task.clone().start(
            async move { client.multiplex_connection(stream).await },
            move |res| async move {
                match res {
                    Ok(()) => info!("[IRC SERVER] Disconnected client from {}", peer_addr),
                    Err(e) => error!("[IRC SERVER] Disconnected client from {}: {}", peer_addr, e),
                }

                self.clone().clients.lock().await.remove(&port);
            },
            Error::ChannelStopped,
            ex,
        );

        Ok(())
    }

    fn pad(string: &str) -> Vec<u8> {
        let mut bytes = string.as_bytes().to_vec();
        bytes.resize(MAX_NICK_LEN, 0x00);
        bytes
    }

    fn unpad(vec: &mut Vec<u8>) {
        if let Some(i) = vec.iter().rposition(|x| *x != 0) {
            let new_len = i + 1;
            vec.truncate(new_len);
        }
    }

    /// Try encrypting a given `Privmsg` if there is such a channel/contact.
    pub async fn try_encrypt<T: Priv>(&self, privmsg: &mut T) {
        if let Some((name, channel)) = self.channels.read().await.get_key_value(privmsg.channel()) {
            if let Some(saltbox) = &channel.saltbox {
                // We will pad the name and nick to MAX_NICK_LEN so they all look the same.
                *privmsg.channel() = saltbox::encrypt(saltbox, &Self::pad(privmsg.channel()));
                *privmsg.nick() = saltbox::encrypt(saltbox, &Self::pad(privmsg.nick()));
                *privmsg.msg() = saltbox::encrypt(saltbox, privmsg.msg().as_bytes());
                debug!("Successfully encrypted message for {}", name);
                return
            }
        };

        if let Some((name, contact)) = self.contacts.read().await.get_key_value(privmsg.channel()) {
            if let Some(saltbox) = &contact.saltbox {
                // We will use dummy channel and nick values since they are not used.
                // We don't need to pad them since everyone is using the same ones.
                *privmsg.channel() = saltbox::encrypt(saltbox, b"channel");
                *privmsg.nick() = saltbox::encrypt(saltbox, b"nick");
                *privmsg.msg() = saltbox::encrypt(saltbox, privmsg.msg().as_bytes());
                debug!("Successfully encrypted message for {}", name);
            }
        };
    }

    /// Try decrypting a given potentially encrypted `Privmsg` object.
    pub async fn try_decrypt(&self, privmsg: &mut Privmsg) {
        // If all fields have base58, then we can consider decrypting.
        let channel_ciphertext = match bs58::decode(&privmsg.channel).into_vec() {
            Ok(v) => v,
            Err(_) => return,
        };

        let nick_ciphertext = match bs58::decode(&privmsg.nick).into_vec() {
            Ok(v) => v,
            Err(_) => return,
        };

        let msg_ciphertext = match bs58::decode(&privmsg.msg).into_vec() {
            Ok(v) => v,
            Err(_) => return,
        };

        // Now go through all 3 ciphertexts. We'll use intermediate buffers
        // for decryption, iff all passes, we will return a modified
        // (i.e. decrypted) privmsg, otherwise we return the original.
        for (name, channel) in self.channels.read().await.iter() {
            let Some(saltbox) = &channel.saltbox else { continue };

            let Some(mut channel_dec) = saltbox::try_decrypt(saltbox, &channel_ciphertext) else {
                continue
            };

            let Some(mut nick_dec) = saltbox::try_decrypt(saltbox, &nick_ciphertext) else {
                continue
            };

            let Some(msg_dec) = saltbox::try_decrypt(saltbox, &msg_ciphertext) else { continue };

            Self::unpad(&mut channel_dec);
            Self::unpad(&mut nick_dec);

            privmsg.channel = name.to_string();
            privmsg.nick = String::from_utf8_lossy(&nick_dec).into();
            privmsg.msg = String::from_utf8_lossy(&msg_dec).into();
            debug!("Successfully decrypted message for {}", name);
            return
        }

        for (name, contact) in self.contacts.read().await.iter() {
            let Some(saltbox) = &contact.saltbox else { continue };

            let Some(mut channel_dec) = saltbox::try_decrypt(saltbox, &channel_ciphertext) else {
                continue
            };

            let Some(mut nick_dec) = saltbox::try_decrypt(saltbox, &nick_ciphertext) else {
                continue
            };

            let Some(msg_dec) = saltbox::try_decrypt(saltbox, &msg_ciphertext) else { continue };

            Self::unpad(&mut channel_dec);
            Self::unpad(&mut nick_dec);

            privmsg.channel = name.to_string();
            privmsg.nick = name.to_string();
            privmsg.msg = String::from_utf8_lossy(&msg_dec).into();
            debug!("Successfully decrypted message from {}", name);
            return
        }
    }
}
