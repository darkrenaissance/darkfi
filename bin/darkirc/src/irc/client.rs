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

use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst},
        Arc,
    },
};

use darkfi::{
    event_graph::{proto::EventPut, Event, NULL_ID},
    system::Subscription,
    Error, Result,
};
use darkfi_serial::{deserialize_async_partial, serialize_async};
use futures::FutureExt;
use log::{debug, error, warn};
use smol::{
    io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader},
    lock::{OnceCell, RwLock},
    net::SocketAddr,
    prelude::{AsyncRead, AsyncWrite},
};

use super::{
    server::{IrcServer, MAX_NICK_LEN},
    NickServ, Privmsg, SERVER_NAME,
};

const PENALTY_LIMIT: usize = 5;

/// Reply types, we can either send server replies, or client replies.
pub enum ReplyType {
    /// Server reply, we have to use numerics
    Server((u16, String)),
    /// Client reply, message from someone to some{one,where}
    Client((String, String)),
    /// Pong reply, we just use server origin
    Pong(String),
    /// CAP reply
    Cap(String),
    /// NOTICE reply (from, to, what)
    Notice((String, String, String)),
}

/// Stateful IRC client handler, used for each client connection
pub struct Client {
    /// Pointer to parent `IrcServer`
    pub server: Arc<IrcServer>,
    /// Subscription for incoming events
    pub incoming: Subscription<Event>,
    /// Client socket addr
    pub addr: SocketAddr,
    /// ID of the last sent event
    pub last_sent: RwLock<blake3::Hash>,
    /// Active (joined) channels for this client
    pub channels: RwLock<HashSet<String>>,
    /// Penalty counter, when limit is reached, disconnect client
    pub penalty: AtomicUsize,
    /// Registration marker
    pub registered: AtomicBool,
    /// Registration pause marker
    pub reg_paused: AtomicBool,
    /// Client username
    pub username: Arc<RwLock<String>>,
    /// Client nickname
    pub nickname: Arc<RwLock<String>>,
    /// Client realname
    pub realname: RwLock<String>,
    /// Client caps
    pub caps: RwLock<HashMap<String, bool>>,
    /// Set of seen messages for the user
    /// TODO: It grows indefinitely, needs to be pruned.
    pub seen: OnceCell<sled::Tree>,
    /// NickServ instance
    pub nickserv: Arc<NickServ>,
}

impl Client {
    /// Instantiate a new Client.
    pub async fn new(
        server: Arc<IrcServer>,
        incoming: Subscription<Event>,
        addr: SocketAddr,
    ) -> Result<Self> {
        let caps = HashMap::from([("no-history".to_string(), false)]);

        let username = Arc::new(RwLock::new(String::from("*")));
        let nickname = Arc::new(RwLock::new(String::from("*")));

        Ok(Self {
            server: server.clone(),
            incoming,
            addr,
            last_sent: RwLock::new(NULL_ID),
            channels: RwLock::new(HashSet::new()),
            penalty: AtomicUsize::new(0),
            registered: AtomicBool::new(false),
            reg_paused: AtomicBool::new(false),
            username: username.clone(),
            nickname: nickname.clone(),
            realname: RwLock::new(String::from("*")),
            caps: RwLock::new(caps),
            seen: OnceCell::new(),
            nickserv: Arc::new(NickServ::new(username.clone(), nickname.clone(), server.clone())),
        })
    }

    /// This function handles a single IRC client. We listen to messages from the
    /// IRC client and relay them to the network, and we also get notified of
    /// incoming messages and relay them to the IRC client. The notifications come
    /// from events being inserted into the Event Graph.
    pub async fn multiplex_connection<S>(&self, stream: S) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let (reader, mut writer) = io::split(stream);
        let mut reader = BufReader::new(reader);

        // Our buffer for the client line
        let mut line = String::new();

        loop {
            futures::select! {
                // Process message from the IRC client
                r = reader.read_line(&mut line).fuse() => {
                    // If something failed during reading, we disconnect.
                    if let Err(e) = r {
                        error!("[IRC CLIENT] Read failed for {}: {}", self.addr, e);
                        self.incoming.unsubscribe().await;
                        return Err(Error::ChannelStopped)
                    }

                    // If the penalty limit is reached, disconnect the client.
                    if self.penalty.load(SeqCst) == PENALTY_LIMIT {
                        self.incoming.unsubscribe().await;
                        return Err(Error::ChannelStopped)
                    }

                    // We'll be strict here and disconnect the client
                    // in case line processing failed in any way.
                    match self.process_client_line(&line, &mut writer).await {
                        // If we got an event back, we should broadcast it.
                        // This means we add it to our DAG, and the DAG will
                        // handle the rest of the propagation.
                        Ok(Some(event)) => {
                            // Update the last sent event.
                            let event_id = event.id();
                            *self.last_sent.write().await = event_id;

                            // If it fails for some reason, for now, we just note it
                            // and pass.
                            if let Err(e) = self.server.darkirc.event_graph.dag_insert(&[event.clone()]).await {
                                error!("[IRC CLIENT] Failed inserting new event to DAG: {}", e);
                            } else {
                                // We sent this, so it should be considered seen.
                                if let Err(e) = self.mark_seen(&event_id).await {
                                    error!("[IRC CLIENT] (multiplex_connection) self.mark_seen({}) failed: {}", event_id, e);
                                    return Err(e)
                                }

                                // Otherwise, broadcast it
                                self.server.darkirc.p2p.broadcast(&EventPut(event)).await;
                            }
                        }

                        // If we got nothing, we just pass.
                        Ok(None) => {}

                        // If we got an error, we disconnect the client.
                        Err(e) => {
                            self.incoming.unsubscribe().await;
                            return Err(e)
                        }
                    }

                    // Clear the line buffer
                    line = String::new();
                    continue
                }

                // Process message from the network. These should only be PRIVMSG.
                r = self.incoming.receive().fuse() => {
                    // We will skip this if it's our own message.
                    let event_id = r.id();
                    if *self.last_sent.read().await == event_id {
                        continue
                    }

                    // If this event was seen, skip it
                    match self.is_seen(&event_id).await {
                        Ok(true) => continue,
                        Ok(false) => {},
                        Err(e) => {
                            error!("[IRC CLIENT] (multiplex_connection) self.is_seen({}) failed: {}", event_id, e);
                            return Err(e)
                        }
                    }

                    // Try to deserialize the `Event`'s content into a `Privmsg`
                    let mut privmsg: Privmsg = match deserialize_async_partial(r.content()).await {
                        Ok((v, _)) => v,
                        Err(e) => {
                            error!("[IRC CLIENT] Failed deserializing incoming Privmsg event: {}", e);
                            continue
                        }
                    };

                    // We should skip any attempts to contact services from the network.
                    if ["nickserv", "chanserv"].contains(&privmsg.nick.to_lowercase().as_str()) {
                        continue
                    }

                    // If successful, potentially decrypt it:
                    self.server.try_decrypt(&mut privmsg).await;

                    // If we have this channel, or it's a DM, forward it to the client.
                    // As a DM, we consider something that is <= MAX_NICK_LEN, and does not
                    // start with the '#' character. With ChaCha, the ciphertext should be
                    // longer than our MAX_NICK_LEN, so in case it is garbled, it should be
                    // skipped by this code.
                    let have_channel = self.channels.read().await.contains(&privmsg.channel);
                    let msg_for_self = !privmsg.channel.starts_with('#') && privmsg.channel.as_bytes().len() <= MAX_NICK_LEN;

                    if have_channel || msg_for_self {
                        // Add the nickname to the list of nicks on the channel, if it's a channel.
                        let mut chans_lock = self.server.channels.write().await;
                        if let Some(chan) = chans_lock.get_mut(&privmsg.channel) {
                            chan.nicks.insert(privmsg.nick.clone());
                        }
                        drop(chans_lock);

                        // Format the message
                        let msg = format!("PRIVMSG {} :{}", privmsg.channel, privmsg.msg);

                        // Send it to the client
                        let reply = ReplyType::Client((privmsg.nick, msg));
                        if let Err(e) = self.reply(&mut writer, &reply).await {
                            error!("[IRC CLIENT] Failed writing PRIVMSG to client: {}", e);
                            continue
                        }

                        // Mark the message as seen for this USER
                        if let Err(e) = self.mark_seen(&event_id).await {
                            error!("[IRC CLIENT] (multiplex_connection) self.mark_seen({}) failed: {}", event_id, e);
                            return Err(e)
                        }
                    }
                }
            }
        }
    }

    /// Send a reply to the IRC client. Matches on the reply type.
    async fn reply<W>(&self, writer: &mut W, reply: &ReplyType) -> Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let r = match reply {
            ReplyType::Server((rpl, msg)) => format!(":{} {:03} {}", SERVER_NAME, rpl, msg),
            ReplyType::Client((nick, msg)) => format!(":{}!~anon@darkirc {}", nick, msg),
            ReplyType::Pong(origin) => format!(":{} PONG :{}", SERVER_NAME, origin),
            ReplyType::Cap(msg) => format!(":{} {}", SERVER_NAME, msg),
            ReplyType::Notice((src, dst, msg)) => {
                format!(":{}!~anon@darkirc NOTICE {} :{}", src, dst, msg)
            }
        };

        debug!("[{}] <-- {}", self.addr, r);

        writer.write(r.as_bytes()).await?;
        writer.write(b"\r\n").await?;
        writer.flush().await?;

        Ok(())
    }

    /// Handle the incoming line given sent by the IRC client
    async fn process_client_line<W>(&self, line: &str, writer: &mut W) -> Result<Option<Event>>
    where
        W: AsyncWrite + Unpin,
    {
        if line.is_empty() || line == "\n" || line == "\r\n" {
            return Err(Error::ParseFailed("Line is empty"))
        }

        let mut line = line.to_string();

        // Remove CRLF
        if &line[(line.len() - 2)..] == "\r\n" {
            line.pop();
            line.pop();
        } else if &line[(line.len() - 1)..] == "\n" {
            line.pop();
        } else {
            return Err(Error::ParseFailed("Line doesn't end with CR/LF"))
        }

        // Parse the line
        let mut tokens = line.split_ascii_whitespace();
        // Commands can begin with :garbage, but we will reject clients
        // doing that for now to keep the protocol simple and focused.
        let cmd = tokens.next().ok_or(Error::ParseFailed("Invalid command line"))?;
        let args = line.replacen(cmd, "", 1);
        let cmd = cmd.to_uppercase();

        debug!("[{}] --> {}{}", self.addr, cmd, args);

        // Handle the command. These implementations are in `command.rs`.
        let replies: Vec<ReplyType> = match cmd.as_str() {
            "ADMIN" => self.handle_cmd_admin(&args).await?,
            "CAP" => self.handle_cmd_cap(&args).await?,
            "INFO" => self.handle_cmd_info(&args).await?,
            "JOIN" => self.handle_cmd_join(&args).await?,
            "LIST" => self.handle_cmd_list(&args).await?,
            "MODE" => self.handle_cmd_mode(&args).await?,
            "MOTD" => self.handle_cmd_motd(&args).await?,
            "NAMES" => self.handle_cmd_names(&args).await?,
            "NICK" => self.handle_cmd_nick(&args).await?,
            "PART" => self.handle_cmd_part(&args).await?,
            "PING" => self.handle_cmd_ping(&args).await?,
            "PRIVMSG" => self.handle_cmd_privmsg(&args).await?,
            "REHASH" => self.handle_cmd_rehash(&args).await?,
            "TOPIC" => self.handle_cmd_topic(&args).await?,
            "USER" => self.handle_cmd_user(&args).await?,
            "VERSION" => self.handle_cmd_version(&args).await?,
            "QUIT" => return Err(Error::ChannelStopped),
            _ => {
                warn!("[IRC CLIENT] Unimplemented \"{}\" command", cmd);
                vec![]
            }
        };

        // Depending on the reply type, we send according messages.
        for reply in replies.iter() {
            self.reply(writer, reply).await?;
        }

        // If the command was a PRIVMSG the client sent, we need to encrypt it and
        // create an Event to broadcast and return it from this function. So let's try.
        // We also do not allow sending unencrypted DMs. In that case, we send a notice
        // to the client to inform them that the feature is not enabled.

        // NOTE: This is not the most performant way to do this, probably not even
        // TODO: the best place to do it. Patches welcome. It's also a bit fragile
        // since we assume that `handle_cmd_privmsg()` won't return any replies.
        if cmd.as_str() == "PRIVMSG" && replies.is_empty() {
            let channel = args.split_ascii_whitespace().next().unwrap().to_string();
            let msg_offset = args.find(':').unwrap() + 1;
            let (_, msg) = args.split_at(msg_offset);
            let mut privmsg = Privmsg {
                channel,
                nick: self.nickname.read().await.to_string(),
                msg: msg.to_string(),
            };

            // Encrypt the Privmsg if an encryption method is available.
            self.server.try_encrypt(&mut privmsg).await;

            // Build a DAG event and return it.
            let event =
                Event::new(serialize_async(&privmsg).await, &self.server.darkirc.event_graph).await;

            return Ok(Some(event))
        }

        Ok(None)
    }

    /// Atomically mark a message as seen for this client.
    pub async fn mark_seen(&self, event_id: &blake3::Hash) -> Result<()> {
        let db = self
            .seen
            .get_or_init(|| async {
                let u = self.username.read().await.to_string();
                self.server.darkirc.sled.open_tree(format!("darkirc_user_{}", u)).unwrap()
            })
            .await;

        debug!("Marking event {} as seen", event_id);
        let mut batch = sled::Batch::default();
        batch.insert(event_id.as_bytes(), &[]);
        Ok(db.apply_batch(batch)?)
    }

    /// Check if a message was already marked seen for this client.
    pub async fn is_seen(&self, event_id: &blake3::Hash) -> Result<bool> {
        let db = self
            .seen
            .get_or_init(|| async {
                let u = self.username.read().await.to_string();
                self.server.darkirc.sled.open_tree(format!("darkirc_user_{}", u)).unwrap()
            })
            .await;

        Ok(db.contains_key(event_id.as_bytes())?)
    }
}
