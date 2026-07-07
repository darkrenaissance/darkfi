/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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
    io::{BufRead, BufReader},
    path::PathBuf,
    sync::Arc,
};

use darkfi::{
    event_graph::Event,
    system::{StoppableTask, StoppableTaskPtr, Subscription},
    util::path::expand_path,
    Error, Result,
};
use darkfi_serial::{deserialize_async, serialize_async};
use futures_rustls::{
    rustls::{
        self,
        pki_types::{CertificateDer, PrivateKeyDer},
    },
    TlsAcceptor,
};
use sled_overlay::sled;
use smol::{
    fs,
    lock::{Mutex, RwLock},
    net::{SocketAddr, TcpListener},
    prelude::{AsyncRead, AsyncWrite},
    Executor,
};
use tracing::{debug, error, info, warn};
use url::Url;

use super::{
    client::Client,
    services::nickserv::{ACCOUNTS_DB_PREFIX, ACCOUNTS_DEFAULT_TREE, ACCOUNTS_KEY_RLN_IDENTITY},
    IrcChannel, IrcContact,
};
use crate::{
    crypto::{rln::RlnIdentity, saltbox},
    pad,
    settings::{parse_autojoin_channels, parse_configured_channels, parse_configured_contacts},
    unpad, DarkIrc, Privmsg,
};

/// Max channel/nick length
pub const MAX_NICK_LEN: usize = 24;

/// Max message length
pub const MAX_MSG_LEN: usize = 512;

/// Result of attempting to reserve the next RLN message slot.
pub enum RlnMessageReservation {
    /// No active RLN identity is configured.
    MissingIdentity,
    /// The active identity has already used its epoch budget.
    BudgetExhausted,
    /// A message slot was persisted and can be used to build a proof.
    Reserved { identity: RlnIdentity, message_id: u64 },
}

/// Persist the active RLN counter to the default mirror and matching account tree.
async fn persist_rln_identity_counter(sled_db: &sled::Db, identity: &RlnIdentity) -> Result<()> {
    let encoded = serialize_async(identity).await;
    let active_commitment = identity.commitment();
    let mut updated_account = false;

    for raw in sled_db.tree_names() {
        let bytes: &[u8] = raw.as_ref();
        let Ok(name) = std::str::from_utf8(bytes) else { continue };
        let Some(account_name) = name.strip_prefix(ACCOUNTS_DB_PREFIX) else { continue };
        if account_name == "default" || account_name.is_empty() {
            continue
        }

        let tree = sled_db.open_tree(name)?;
        let Some(blob) = tree.get(ACCOUNTS_KEY_RLN_IDENTITY)? else { continue };
        let Ok(stored): std::result::Result<RlnIdentity, _> = deserialize_async(&blob).await else {
            continue
        };
        if stored.commitment() == active_commitment {
            tree.insert(ACCOUNTS_KEY_RLN_IDENTITY, encoded.clone())?;
            updated_account = true;
        }
    }

    if !updated_account {
        warn!(
            target: "darkirc::irc::server",
            "active RLN identity has no matching account tree; persisting default mirror only",
        );
    }

    let default_db = sled_db.open_tree(ACCOUNTS_DEFAULT_TREE)?;
    default_db.insert(ACCOUNTS_KEY_RLN_IDENTITY, encoded)?;
    sled_db.flush_async().await?;
    Ok(())
}

/// Reserve the next RLN message ID and persist it before proof creation.
pub(crate) async fn reserve_rln_message_id_in_store(
    sled_db: &sled::Db,
    active: &mut Option<RlnIdentity>,
    now_millis: u64,
) -> Result<RlnMessageReservation> {
    let Some(current) = active else { return Ok(RlnMessageReservation::MissingIdentity) };

    let mut updated = *current;
    let Some(message_id) = updated.next_message_id(now_millis) else {
        return Ok(RlnMessageReservation::BudgetExhausted)
    };

    persist_rln_identity_counter(sled_db, &updated).await?;
    *current = updated;

    Ok(RlnMessageReservation::Reserved { identity: updated, message_id })
}

fn parse_tls_secret<R>(reader: &mut R) -> Result<PrivateKeyDer<'static>>
where
    R: BufRead,
{
    let key = rustls_pemfile::pkcs8_private_keys(reader)
        .next()
        .ok_or(Error::ParseFailed("TLS key missing PKCS#8 private key"))?
        .map_err(|_| Error::ParseFailed("TLS key contains invalid PKCS#8 private key"))?;

    Ok(PrivateKeyDer::Pkcs8(key))
}

fn parse_tls_cert<R>(reader: &mut R) -> Result<CertificateDer<'static>>
where
    R: BufRead,
{
    rustls_pemfile::certs(reader)
        .next()
        .ok_or(Error::ParseFailed("TLS certificate missing"))?
        .map_err(|_| Error::ParseFailed("TLS certificate contains invalid DER"))
}

fn tls_acceptor_from_pem<CR, KR>(
    cert_reader: &mut CR,
    secret_reader: &mut KR,
) -> Result<TlsAcceptor>
where
    CR: BufRead,
    KR: BufRead,
{
    let secret = parse_tls_secret(secret_reader)?;
    let cert = parse_tls_cert(cert_reader)?;
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert], secret)
        .map_err(|_| Error::ParseFailed("TLS certificate and key are invalid"))?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}

fn load_tls_acceptor(tls_cert: &str, tls_secret: &str) -> Result<TlsAcceptor> {
    let f = File::open(expand_path(tls_secret)?)?;
    let mut secret_reader = BufReader::new(f);

    let f = File::open(expand_path(tls_cert)?)?;
    let mut cert_reader = BufReader::new(f);

    tls_acceptor_from_pem(&mut cert_reader, &mut secret_reader)
}

async fn load_default_rln_identity(sled_db: &sled::Db) -> Result<Option<RlnIdentity>> {
    let default_db = sled_db.open_tree(ACCOUNTS_DEFAULT_TREE)?;
    let Some(blob) = default_db.get(ACCOUNTS_KEY_RLN_IDENTITY)? else {
        if default_db.is_empty() {
            return Ok(None)
        }

        return Err(Error::ParseFailed("Default RLN account is missing identity record"))
    };

    let identity: RlnIdentity = deserialize_async(&blob)
        .await
        .map_err(|_| Error::ParseFailed("Default RLN account identity is corrupted"))?;

    Ok(Some(identity))
}

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
    /// Static-DAG events whose broadcast is deferred until the
    /// EventGraph is synced.
    pub pending_static_broadcasts: Mutex<Vec<(Event, Vec<u8>)>>,
    /// Active client connections
    clients: Mutex<HashMap<u16, StoppableTaskPtr>>,
    /// IRC server Password
    pub password: String,
}

impl IrcServer {
    /// Reserve and persist the next RLN message slot before proof creation.
    pub async fn reserve_rln_message_id(&self, now_millis: u64) -> Result<RlnMessageReservation> {
        let mut active = self.rln_identity.write().await;
        reserve_rln_message_id_in_store(&self.darkirc.sled, &mut active, now_millis).await
    }

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
                let (Some(tls_cert), Some(tls_secret)) = (tls_cert.as_ref(), tls_secret.as_ref())
                else {
                    return Err(Error::ParseFailed("TLS certificate and key are required"))
                };

                Some(load_tls_acceptor(tls_cert, tls_secret)?)
            }
            _ => None,
        };

        // Set the default RLN account if any. When RLN is disabled, avoid
        // loading account state that cannot affect outbound messages.
        let rln_identity = if darkirc.event_graph.rln_enabled() {
            let rln_identity = load_default_rln_identity(&darkirc.sled).await?;
            if rln_identity.is_some() {
                info!("Default RLN account set");
            }
            rln_identity
        } else {
            info!("RLN disabled; skipping default RLN account load");
            None
        };

        let self_ = Arc::new(Self {
            darkirc,
            config_path,
            listener,
            acceptor,
            autojoin: RwLock::new(Vec::new()),
            channels: RwLock::new(HashMap::new()),
            contacts: RwLock::new(HashMap::new()),
            rln_identity: RwLock::new(rln_identity),
            pending_static_broadcasts: Mutex::new(Vec::new()),
            clients: Mutex::new(HashMap::new()),
            password,
        });

        // Load any channel/contact configuration.
        self_.rehash().await?;

        Ok(self_)
    }

    /// Drain `pending_static_broadcasts` and broadcast each entry.
    pub async fn drain_pending_static_broadcasts(&self) -> Result<usize> {
        let drained: Vec<(Event, Vec<u8>)> = {
            let mut guard = self.pending_static_broadcasts.lock().await;
            std::mem::take(&mut *guard)
        };
        let n = drained.len();
        for (event, blob) in drained {
            self.darkirc.event_graph.static_broadcast(event, blob).await?;
        }
        Ok(n)
    }

    /// Reload the darkirc configuration file and reconfigure channels and contacts.
    pub async fn rehash(&self) -> Result<()> {
        let contents = fs::read_to_string(&self.config_path).await?;
        let contents = match toml::from_str(&contents) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed parsing TOML config: {e}");
                return Err(Error::ParseFailed("Failed parsing TOML config"))
            }
        };

        // Parse autojoin channels
        let autojoin = parse_autojoin_channels(&contents)?;

        // Parse configured channels
        let configured_channels = parse_configured_channels(&contents)?;

        // Parse configured contacts
        let contacts = parse_configured_contacts(&contents)?;

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

        Ok(())
    }

    /// Start accepting new IRC connections.
    pub async fn listen(self: Arc<Self>, ex: Arc<Executor<'_>>) -> Result<()> {
        loop {
            let (stream, peer_addr) = match self.listener.accept().await {
                Ok((s, a)) => (s, a),

                // As per usual accept(2) recommendations
                Err(e)
                    if matches!(
                        e.raw_os_error(),
                        Some(libc::EAGAIN | libc::ECONNABORTED | libc::EPROTO | libc::EINTR)
                    ) =>
                {
                    continue
                }

                Err(e) => {
                    error!("[IRC SERVER] Failed accepting new connection: {e}");
                    continue
                }
            };

            match &self.acceptor {
                // Expecting encrypted TLS connection
                Some(acceptor) => {
                    let stream = match acceptor.accept(stream).await {
                        Ok(s) => s,
                        Err(e) => {
                            error!("[IRC SERVER] Failed accepting new TLS connection: {e}");
                            continue
                        }
                    };

                    // Subscribe to incoming events and set up the connection.
                    let incoming = self.darkirc.event_graph.event_pub.clone().subscribe().await;
                    let incoming_st = self.darkirc.event_graph.static_pub.clone().subscribe().await;
                    if let Err(e) = self
                        .clone()
                        .process_connection(stream, peer_addr, incoming, incoming_st, ex.clone())
                        .await
                    {
                        error!("[IRC SERVER] Failed processing new connection: {e}");
                        continue
                    };
                }

                // Expecting plain TCP connection
                None => {
                    // Subscribe to incoming events and set up the connection.
                    let incoming = self.darkirc.event_graph.event_pub.clone().subscribe().await;
                    let incoming_st = self.darkirc.event_graph.static_pub.clone().subscribe().await;
                    if let Err(e) = self
                        .clone()
                        .process_connection(stream, peer_addr, incoming, incoming_st, ex.clone())
                        .await
                    {
                        error!("[IRC SERVER] Failed processing new connection: {e}");
                        continue
                    };
                }
            }

            info!("[IRC SERVER] Accepted new client connection at: {peer_addr}");
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
        incoming_st: Subscription<Event>,
        ex: Arc<Executor<'_>>,
    ) -> Result<()> {
        let port = peer_addr.port();
        let client = Client::new(self.clone(), incoming, incoming_st, peer_addr).await?;

        let conn_task = StoppableTask::new();
        self.clients.lock().await.insert(port, conn_task.clone());

        conn_task.clone().start(
            async move { client.multiplex_connection(stream).await },
            move |res| async move {
                match res {
                    Ok(()) => info!("[IRC SERVER] Disconnected client from {peer_addr}"),
                    Err(e) => error!("[IRC SERVER] Disconnected client from {peer_addr}: {e}"),
                }

                self.clone().clients.lock().await.remove(&port);
            },
            Error::ChannelStopped,
            ex,
        );

        Ok(())
    }

    /// Try encrypting a given `Privmsg` if there is such a channel/contact.
    pub async fn try_encrypt(&self, privmsg: &mut Privmsg) {
        if let Some((name, channel)) = self.channels.read().await.get_key_value(&privmsg.channel) {
            if let Some(saltbox) = &channel.saltbox {
                // We will use a dummy channel value of MAX_NICK_LEN,
                // since its not used, so all encrypted messages look the same.
                privmsg.channel = saltbox::encrypt(saltbox, &[0x00; MAX_NICK_LEN]);
                // We will pad the name to MAX_NICK_LEN so they all look the same
                privmsg.nick = saltbox::encrypt(saltbox, &pad(&privmsg.nick));
                privmsg.msg = saltbox::encrypt(saltbox, privmsg.msg.as_bytes());
                debug!("Successfully encrypted message for {name}");
                return
            }
        };

        if let Some((name, contact)) = self.contacts.read().await.get_key_value(&privmsg.channel) {
            // We will use dummy channel and nick values of MAX_NICK_LEN,
            // since they are not used, so all encrypted messages look the same.
            privmsg.channel = saltbox::encrypt(&contact.saltbox, &[0x00; MAX_NICK_LEN]);
            // We will encrypt the dummy nick value using our own self saltbox,
            // so we can identify our messages.
            privmsg.nick = saltbox::encrypt(&contact.self_saltbox, &[0x00; MAX_NICK_LEN]);
            privmsg.msg = saltbox::encrypt(&contact.saltbox, privmsg.msg.as_bytes());
            debug!("Successfully encrypted message for {name}");
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

            unpad(&mut nick_dec);

            privmsg.channel = name.to_string();
            privmsg.nick = String::from_utf8_lossy(&nick_dec).into();
            privmsg.msg = String::from_utf8_lossy(&msg_dec).into();
            debug!("Successfully decrypted message for {name}");
            return
        }

        for (name, contact) in self.contacts.read().await.iter() {
            if saltbox::try_decrypt(&contact.saltbox, &channel_ciphertext).is_none() {
                continue
            };

            // Since everyone encrypts the dummy nick value with their self saltbox,
            // we try to decrypt using our, to identify our messages.
            let nick = if saltbox::try_decrypt(&contact.self_saltbox, &nick_ciphertext).is_some() {
                String::from(self_nickname)
            } else {
                name.to_string()
            };

            let Some(msg_dec) = saltbox::try_decrypt(&contact.saltbox, &msg_ciphertext) else {
                warn!(target: "darkirc::irc::server::try_decrypt", "Could not decrypt message ciphertext for contact: {name}");
                continue
            };

            privmsg.channel = name.to_string();
            privmsg.nick = nick;
            privmsg.msg = String::from_utf8_lossy(&msg_dec).into();
            debug!("Successfully decrypted message from {name}");
            return
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use darkfi::{event_graph::rln::epoch_of, Error};
    use darkfi_sdk::pasta::pallas;
    use darkfi_serial::deserialize_async;

    use super::*;

    fn test_identity(limit: u64) -> RlnIdentity {
        RlnIdentity {
            nullifier: pallas::Base::from(0xabc_u64),
            trapdoor: pallas::Base::from(0xdef_u64),
            user_message_limit: limit,
            message_id: 0,
            last_epoch: 0,
        }
    }

    #[test]
    fn tls_secret_parser_rejects_malformed_key() {
        let mut reader = Cursor::new(b"not a private key".as_slice());

        assert!(matches!(parse_tls_secret(&mut reader), Err(Error::ParseFailed(_))));
    }

    #[test]
    fn tls_cert_parser_rejects_malformed_cert() {
        let mut reader = Cursor::new(b"not a certificate".as_slice());

        assert!(matches!(parse_tls_cert(&mut reader), Err(Error::ParseFailed(_))));
    }

    #[test]
    fn load_default_rln_identity_returns_none_for_empty_tree() {
        smol::block_on(async {
            let sled_db = sled::Config::new().temporary(true).open().unwrap();

            let identity = load_default_rln_identity(&sled_db).await.unwrap();

            assert!(identity.is_none());
        })
    }

    #[test]
    fn load_default_rln_identity_rejects_missing_identity_record() {
        smol::block_on(async {
            let sled_db = sled::Config::new().temporary(true).open().unwrap();
            let default = sled_db.open_tree(ACCOUNTS_DEFAULT_TREE).unwrap();
            default.insert(b"other", b"value").unwrap();

            let err = match load_default_rln_identity(&sled_db).await {
                Ok(_) => panic!("expected missing identity record error"),
                Err(e) => e,
            };

            assert!(matches!(
                err,
                Error::ParseFailed("Default RLN account is missing identity record")
            ));
        })
    }

    #[test]
    fn load_default_rln_identity_rejects_corrupted_identity_record() {
        smol::block_on(async {
            let sled_db = sled::Config::new().temporary(true).open().unwrap();
            let default = sled_db.open_tree(ACCOUNTS_DEFAULT_TREE).unwrap();
            default.insert(ACCOUNTS_KEY_RLN_IDENTITY, b"not an identity").unwrap();

            let err = match load_default_rln_identity(&sled_db).await {
                Ok(_) => panic!("expected corrupted identity record error"),
                Err(e) => e,
            };

            assert!(matches!(err, Error::ParseFailed("Default RLN account identity is corrupted")));
        })
    }

    #[test]
    fn rln_message_reservation_persists_default_and_account_counters() {
        smol::block_on(async {
            let sled_db = sled::Config::new().temporary(true).open().unwrap();
            let account = sled_db.open_tree(format!("{ACCOUNTS_DB_PREFIX}alice")).unwrap();
            let default = sled_db.open_tree(ACCOUNTS_DEFAULT_TREE).unwrap();
            let identity = test_identity(2);
            let encoded = serialize_async(&identity).await;
            account.insert(ACCOUNTS_KEY_RLN_IDENTITY, encoded.clone()).unwrap();
            default.insert(ACCOUNTS_KEY_RLN_IDENTITY, encoded).unwrap();

            let now = 1_704_067_800_000;
            let mut active = Some(identity);
            let reservation =
                reserve_rln_message_id_in_store(&sled_db, &mut active, now).await.unwrap();
            let RlnMessageReservation::Reserved { identity: reserved, message_id } = reservation
            else {
                panic!("expected reservation")
            };
            assert_eq!(message_id, 0);
            assert_eq!(reserved.message_id, 1);
            assert_eq!(reserved.last_epoch, epoch_of(now));

            let stored_default: RlnIdentity =
                deserialize_async(&default.get(ACCOUNTS_KEY_RLN_IDENTITY).unwrap().unwrap())
                    .await
                    .unwrap();
            let stored_account: RlnIdentity =
                deserialize_async(&account.get(ACCOUNTS_KEY_RLN_IDENTITY).unwrap().unwrap())
                    .await
                    .unwrap();
            assert_eq!(stored_default.message_id, 1);
            assert_eq!(stored_account.message_id, 1);
            assert_eq!(stored_default.last_epoch, epoch_of(now));
            assert_eq!(stored_account.last_epoch, epoch_of(now));

            let reservation =
                reserve_rln_message_id_in_store(&sled_db, &mut active, now).await.unwrap();
            let RlnMessageReservation::Reserved { message_id, .. } = reservation else {
                panic!("expected second reservation")
            };
            assert_eq!(message_id, 1);

            let exhausted =
                reserve_rln_message_id_in_store(&sled_db, &mut active, now).await.unwrap();
            assert!(matches!(exhausted, RlnMessageReservation::BudgetExhausted));
        })
    }
}
