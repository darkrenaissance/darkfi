/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, Cursor},
    path::PathBuf,
    sync::Arc,
};

use darkfi::{
    event_graph::Event,
    system::{StoppableTask, StoppableTaskPtr, Subscription},
    util::path::expand_path,
    zk::{empty_witnesses, ProvingKey, VerifyingKey, ZkCircuit},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_sdk::crypto::MerkleTree;
use darkfi_serial::serialize_async;
use futures_rustls::{
    rustls::{self, pki_types::PrivateKeyDer},
    TlsAcceptor,
};
use log::{debug, error, info, warn};
use sled_overlay::sled;
use smol::{
    fs,
    lock::{Mutex, RwLock},
    net::{SocketAddr, TcpListener},
    prelude::{AsyncRead, AsyncWrite},
    Executor,
};
use url::Url;

use super::{client::Client, ChaChaBox, IrcChannel, IrcContact, Priv, Privmsg};
use crate::{
    crypto::{
        rln::{RlnIdentity, RLN2_SIGNAL_ZKBIN, RLN2_SLASH_ZKBIN},
        saltbox,
    },
    settings::{
        parse_autojoin_channels, parse_configured_channels, parse_configured_contacts,
        parse_rln_identity,
    },
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
    /// Configured RLN identity
    pub rln_identity: RwLock<Option<RlnIdentity>>,
    /// Saltbox used to encrypt our nick in direct messages
    saltbox: RwLock<Option<Arc<ChaChaBox>>>,
    /// Active client connections
    clients: Mutex<HashMap<u16, StoppableTaskPtr>>,
    /// IRC server Password
    pub password: String,
    /// Persistent server storage
    pub server_store: sled::Tree,
    /// RLN identity storage
    pub rln_identity_store: sled::Tree,
    /// RLN Signal VerifyingKey
    pub rln_signal_vk: VerifyingKey,
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

        // Open persistent dbs
        let server_store = darkirc.sled.open_tree("server_store")?;
        let rln_identity_store = darkirc.sled.open_tree("rln_identity_store")?;

        // Generate RLN proving and verifying keys, if needed
        let rln_signal_zkbin = ZkBinary::decode(RLN2_SIGNAL_ZKBIN)?;
        let rln_signal_circuit =
            ZkCircuit::new(empty_witnesses(&rln_signal_zkbin)?, &rln_signal_zkbin);

        if server_store.get("rlnv2-diff-signal-pk")?.is_none() {
            info!(target: "irc::server", "[RLN] Creating RlnV2_Diff_Signal ProvingKey");
            let provingkey = ProvingKey::build(rln_signal_zkbin.k, &rln_signal_circuit);
            let mut buf = vec![];
            provingkey.write(&mut buf)?;
            server_store.insert("rlnv2-diff-signal-pk", buf)?;
        }

        let rln_signal_vk = match server_store.get("rlnv2-diff-signal-vk")? {
            Some(vk) => {
                let mut reader = Cursor::new(vk);
                VerifyingKey::read(&mut reader, rln_signal_circuit)?
            }
            None => {
                info!(target: "irc::server", "[RLN] Creating RlnV2_Diff_Signal VerifyingKey");
                let verifyingkey = VerifyingKey::build(rln_signal_zkbin.k, &rln_signal_circuit);
                let mut buf = vec![];
                verifyingkey.write(&mut buf)?;
                server_store.insert("rlnv2-diff-signal-vk", buf)?;
                verifyingkey
            }
        };

        if server_store.get("rlnv2-diff-slash-pk")?.is_none() {
            info!(target: "irc::server", "[RLN] Creating RlnV2_Diff_Slash ProvingKey");
            let zkbin = ZkBinary::decode(RLN2_SLASH_ZKBIN)?;
            let circuit = ZkCircuit::new(empty_witnesses(&zkbin).unwrap(), &zkbin);
            let provingkey = ProvingKey::build(zkbin.k, &circuit);
            let mut buf = vec![];
            provingkey.write(&mut buf)?;
            server_store.insert("rlnv2-diff-slash-pk", buf)?;
        }

        if server_store.get("rlnv2-diff-slash-vk")?.is_none() {
            info!(target: "irc::server", "[RLN] Creating RlnV2_Diff_Slash VerifyingKey");
            let zkbin = ZkBinary::decode(RLN2_SIGNAL_ZKBIN)?;
            let circuit = ZkCircuit::new(empty_witnesses(&zkbin).unwrap(), &zkbin);
            let verifyingkey = VerifyingKey::build(zkbin.k, &circuit);
            let mut buf = vec![];
            verifyingkey.write(&mut buf)?;
            server_store.insert("rlnv2-diff-slash-vk", buf)?;
        }

        // Initialize RLN Incremental Merkle tree if necessary
        if server_store.get("rln_identity_tree")?.is_none() {
            let tree = MerkleTree::new(1);
            server_store.insert("rln_identity_tree", serialize_async(&tree).await)?;
        }

        let self_ = Arc::new(Self {
            darkirc,
            config_path,
            listener,
            acceptor,
            autojoin: RwLock::new(Vec::new()),
            channels: RwLock::new(HashMap::new()),
            contacts: RwLock::new(HashMap::new()),
            saltbox: RwLock::new(None),
            rln_identity: RwLock::new(None),
            clients: Mutex::new(HashMap::new()),
            password,
            server_store,
            rln_identity_store,
            rln_signal_vk,
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
        let configured_channels = parse_configured_channels(&contents)?;

        // Parse configured contacts
        let (contacts, saltbox) = parse_configured_contacts(&contents)?;

        // Parse RLN identity
        let rln_identity = parse_rln_identity(&contents)?;

        // Persist unconfigured channels (joined from client, or autojoined without config)
        let channels = {
            let old_channels = self.channels.read().await.clone();
            let unconfigured_channels: HashMap<String, IrcChannel> = old_channels
                .into_iter()
                .filter(|(chan_str, _)| !configured_channels.contains_key(chan_str))
                .collect();
            configured_channels.into_iter().chain(unconfigured_channels).collect()
        };

        // Only if everything is fine, replace.
        *self.autojoin.write().await = autojoin;
        *self.channels.write().await = channels;
        *self.contacts.write().await = contacts;
        *self.saltbox.write().await = saltbox;
        *self.rln_identity.write().await = rln_identity;

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
                // We will use a dummy channel value of MAX_NICK_LEN,
                // since its not used, so all encrypted messages look the same.
                *privmsg.channel() = saltbox::encrypt(saltbox, &[0x00; MAX_NICK_LEN]);
                // We will pad the name to MAX_NICK_LEN so they all look the same
                *privmsg.nick() = saltbox::encrypt(saltbox, &Self::pad(privmsg.nick()));
                *privmsg.msg() = saltbox::encrypt(saltbox, privmsg.msg().as_bytes());
                debug!("Successfully encrypted message for {}", name);
                return
            }
        };

        if let Some((name, contact)) = self.contacts.read().await.get_key_value(privmsg.channel()) {
            if let Some(saltbox) = &contact.saltbox {
                // We will use dummy channel and nick values of MAX_NICK_LEN,
                // since they are not used, so all encrypted messages look the same.
                *privmsg.channel() = saltbox::encrypt(saltbox, &[0x00; MAX_NICK_LEN]);
                // We will encrypt the dummy nick value using our own self saltbox,
                // so we can identify our messages. We can safely unwrap here since
                // we know that if contacts exist, our self saltbox does as well.
                *privmsg.nick() = saltbox::encrypt(
                    self.saltbox.read().await.as_ref().unwrap(),
                    &[0x00; MAX_NICK_LEN],
                );
                *privmsg.msg() = saltbox::encrypt(saltbox, privmsg.msg().as_bytes());
                debug!("Successfully encrypted message for {}", name);
            }
        };
    }

    /// Try decrypting a given potentially encrypted `Privmsg` object.
    pub async fn try_decrypt(&self, privmsg: &mut Privmsg, self_nickname: &str) {
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

            if saltbox::try_decrypt(saltbox, &channel_ciphertext).is_none() {
                continue
            };

            let Some(mut nick_dec) = saltbox::try_decrypt(saltbox, &nick_ciphertext) else {
                warn!(target: "darkirc::irc::server::try_decrypt", "Could not decrypt nick ciphertext for channel: {name}");
                continue
            };

            let Some(msg_dec) = saltbox::try_decrypt(saltbox, &msg_ciphertext) else {
                warn!(target: "darkirc::irc::server::try_decrypt", "Could not decrypt message ciphertext for channel: {name}");
                continue
            };

            Self::unpad(&mut nick_dec);

            privmsg.channel = name.to_string();
            privmsg.nick = String::from_utf8_lossy(&nick_dec).into();
            privmsg.msg = String::from_utf8_lossy(&msg_dec).into();
            debug!("Successfully decrypted message for {}", name);
            return
        }

        for (name, contact) in self.contacts.read().await.iter() {
            let Some(saltbox) = &contact.saltbox else { continue };

            if saltbox::try_decrypt(saltbox, &channel_ciphertext).is_none() {
                continue
            };

            // Since everyone encrypts the dummy nick value with their self saltbox,
            // we try to decrypt using our, to identify our messages. We can safely
            // unwrap here since we know that if contacts exist, our self saltbox does as well.
            let nick = if saltbox::try_decrypt(
                self.saltbox.read().await.as_ref().unwrap(),
                &nick_ciphertext,
            )
            .is_some()
            {
                String::from(self_nickname)
            } else {
                name.to_string()
            };

            let Some(msg_dec) = saltbox::try_decrypt(saltbox, &msg_ciphertext) else {
                warn!(target: "darkirc::irc::server::try_decrypt", "Could not decrypt message ciphertext for contact: {name}");
                continue
            };

            privmsg.channel = name.to_string();
            privmsg.nick = nick;
            privmsg.msg = String::from_utf8_lossy(&msg_dec).into();
            debug!("Successfully decrypted message from {}", name);
            return
        }
    }
}
