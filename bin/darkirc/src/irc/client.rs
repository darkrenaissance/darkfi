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
    collections::{HashMap, HashSet, VecDeque},
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
use darkfi_sdk::crypto::schnorr::SchnorrPublic;
use darkfi_serial::{deserialize, serialize_async};
use futures::FutureExt;
use log::{debug, error, warn};
use sled_overlay::sled;
use smol::{
    io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader},
    lock::{OnceCell, RwLock},
    net::SocketAddr,
    prelude::{AsyncRead, AsyncWrite},
};

use super::{
    server::{IrcServer, MAX_MSG_LEN},
    Modmsg, Msg, NickServ, OldPrivmsg, Privmsg, SERVER_NAME,
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
    /// CAP END marker
    pub is_cap_end: AtomicBool,
    /// Password setup marker
    pub is_pass_set: AtomicBool,
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
        let caps =
            HashMap::from([("no-history".to_string(), false), ("no-autojoin".to_string(), false)]);

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
            is_cap_end: AtomicBool::new(false),
            is_pass_set: AtomicBool::new(false),
            username: username.clone(),
            nickname: nickname.clone(),
            realname: RwLock::new(String::from("*")),
            caps: RwLock::new(caps),
            seen: OnceCell::new(),
            nickserv: Arc::new(
                NickServ::new(username.clone(), nickname.clone(), server.clone()).await?,
            ),
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

        let mut args_queue: VecDeque<_> = VecDeque::new();

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
                    match self.process_client_line(&line, &mut writer, &mut args_queue).await {
                        // If we got an event back, we should broadcast it.
                        // This means we add it to our DAG, and the DAG will
                        // handle the rest of the propagation.
                        Ok(Some(events)) => {
                            for event in events {
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

                    // Try to deserialize the `Event`'s content and handle it
                    // based on its type.
                    let skip = match Msg::deserialize(r.content()).await {
                        Ok(Msg::V1(old_msg)) => self.handle_privmsg(old_msg.into_new(), &mut writer).await,
                        Ok(Msg::V2(new_msg)) => self.handle_privmsg(new_msg, &mut writer).await,
                        Ok(Msg::Mod(mod_msg)) => self.handle_modmsg(mod_msg, &mut writer).await,
                        Err(e) => {
                            error!("[IRC CLIENT] Failed deserializing incoming Privmsg event: {}", e);
                            true
                        }
                    };

                    // Mark the message as seen for this USER if we didn't skip it
                    if !skip {
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
    async fn process_client_line<W>(
        &self,
        line: &str,
        writer: &mut W,
        args_queue: &mut VecDeque<(String, String)>,
    ) -> Result<Option<Vec<Event>>>
    where
        W: AsyncWrite + Unpin,
    {
        if line.trim().is_empty() {
            // Silently ignore empty commands
            return Ok(None)
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

        // Prefix the message part of PRIVMSG with ':' if is not already.
        // Or realname part of USER command.
        let mut words: Vec<String> = line.split_whitespace().map(|s| s.to_string()).collect();
        if words[0].to_uppercase() == "PRIVMSG" {
            if words.len() > 1 && !words[2].starts_with(':') {
                words[2] = format!(":{}", words[2]);
            }
            line = words.join(" ");
        } else if words[0].to_uppercase() == "USER" {
            if words.len() > 1 && !words[4].starts_with(':') {
                words[4] = format!(":{}", words[4]);
            }
            line = words.join(" ");
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
            "JOIN" => self.handle_cmd_join(&args, true).await?,
            "LIST" => self.handle_cmd_list(&args).await?,
            "MODE" => self.handle_cmd_mode(&args).await?,
            "MOTD" => self.handle_cmd_motd(&args).await?,
            "NAMES" => self.handle_cmd_names(&args).await?,
            "NICK" => self.handle_cmd_nick(&args).await?,
            "PART" => self.handle_cmd_part(&args).await?,
            "PASS" => self.handle_cmd_pass(&args).await?,
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
        // TODO: add rest moderation commands here and ensure each one is tested
        let cmd_str = cmd.as_str();
        if (cmd_str == "PRIVMSG" && replies.is_empty()) || cmd_str == "TOPIC" {
            // If the DAG is not synced yet, queue client lines
            // Once synced, send queued lines and continue as normal
            if !*self.server.darkirc.event_graph.synced.read().await {
                debug!("DAG is still syncing, queuing and skipping...");
                args_queue.push_back((cmd, args));
                return Ok(None)
            }

            // Check if we have queued PRIVMSGs, if we do send all of them first.
            let mut pending_events = vec![];
            if !args_queue.is_empty() {
                for _ in 0..args_queue.len() {
                    let (args_cmd, args) = args_queue.pop_front().unwrap();
                    // Grab the event based on the command
                    let event = match args_cmd.as_str() {
                        "PRIVMSG" => self.privmsg_to_event(args).await,
                        "TOPIC" => {
                            let Some(e) = self.topic_to_event(args).await else {
                                continue;
                            };
                            e
                        }
                        _ => continue,
                    };

                    pending_events.push(event);
                }
                return Ok(Some(pending_events))
            }

            // If queue is empty, create an event and return it
            let event = match cmd_str {
                "PRIVMSG" => self.privmsg_to_event(args).await,
                "TOPIC" => {
                    let Some(e) = self.topic_to_event(args).await else {
                        return Ok(None);
                    };
                    e
                }
                _ => return Ok(None),
            };

            return Ok(Some(vec![event]))
        }

        Ok(None)
    }

    // Internal helper function that creates an Event from PRIVMSG arguments
    async fn privmsg_to_event(&self, args: String) -> Event {
        let channel = args.split_ascii_whitespace().next().unwrap().to_string();
        let msg_offset = args.find(':').unwrap() + 1;
        let (_, msg) = args.split_at(msg_offset);

        // Truncate messages longer than MAX_MSG_LEN
        let msg = if msg.len() > MAX_MSG_LEN { msg.split_at(MAX_MSG_LEN).0 } else { msg };

        // TODO: This is kept as old version of privmsg, since now we
        // can deserialize both old and new versions, after some time
        // this will be replaced with Privmsg (new version)
        let mut privmsg = OldPrivmsg {
            channel,
            nick: self.nickname.read().await.to_string(),
            msg: msg.to_string(),
        };

        // Encrypt the Privmsg if an encryption method is available.
        self.server.try_encrypt(&mut privmsg).await;

        // Build a DAG event and return it.
        Event::new(serialize_async(&privmsg).await, &self.server.darkirc.event_graph).await
    }

    // Internal helper function that creates an Event from TOPIC arguments.
    async fn topic_to_event(&self, args: String) -> Option<Event> {
        let channel_name = args.split_ascii_whitespace().next().unwrap().to_string();

        // Check if we have moderation key for this channel
        let channels = self.server.channels.read().await;
        let Some(channel) = channels.get(&channel_name) else {
            drop(channels);
            return None
        };
        let Some(mod_secret_key) = channel.mod_secret_key else {
            drop(channels);
            return None
        };
        drop(channels);

        let topic_offset = args.find(':').unwrap() + 1;
        let (_, topic) = args.split_at(topic_offset);

        // Truncate topic longer than MAX_MSG_LEN
        let topic = if topic.len() > MAX_MSG_LEN { topic.split_at(MAX_MSG_LEN).0 } else { topic };

        // Create the Modmsg
        let mut modmsg =
            Modmsg::new(channel_name, String::from("TOPIC"), String::from(topic), &mod_secret_key);

        // Encrypt the Modmsg if an encryption method is available
        self.server.try_encrypt_modmsg(&mut modmsg).await;

        // Build a DAG event and return it
        Some(Event::new(serialize_async(&modmsg).await, &self.server.darkirc.event_graph).await)
    }

    /// Process provided `Privmsg`.
    /// Returns bool flag indicating if the message should be skipped.
    async fn handle_privmsg<W>(&self, mut privmsg: Privmsg, writer: &mut W) -> bool
    where
        W: AsyncWrite + Unpin,
    {
        // Potentially decrypt it:
        self.server.try_decrypt(&mut privmsg, self.nickname.read().await.as_ref()).await;

        // We should skip any attempts to contact services from the network.
        if ["nickserv", "chanserv"].contains(&privmsg.nick.to_lowercase().as_str()) {
            return true
        }

        // If the privmsg is not intented for any of the given
        // channels or contacts, ignore it.
        let channels = self.channels.read().await;
        let contacts = self.server.contacts.read().await;
        if !channels.contains(&privmsg.channel) && !contacts.contains_key(&privmsg.channel) {
            return true
        }

        // Add the nickname to the list of nicks on the channel, if it's a channel.
        let mut chans_lock = self.server.channels.write().await;
        if let Some(chan) = chans_lock.get_mut(&privmsg.channel) {
            chan.nicks.insert(privmsg.nick.clone());
        }
        drop(chans_lock);

        // Handle message lines individually
        for line in privmsg.msg.lines() {
            // Skip empty lines
            if line.is_empty() {
                continue
            }

            // Format the message
            let msg = format!("PRIVMSG {} :{}", privmsg.channel, line);

            // Send it to the client
            let reply = ReplyType::Client((privmsg.nick.clone(), msg));
            if let Err(e) = self.reply(writer, &reply).await {
                error!("[IRC CLIENT] Failed writing PRIVMSG to client: {}", e);
                continue
            }
        }

        false
    }

    /// Process provided `Modmsg`.
    /// Returns bool flag indicating if the message should be skipped.
    async fn handle_modmsg<W>(&self, mut modmsg: Modmsg, writer: &mut W) -> bool
    where
        W: AsyncWrite + Unpin,
    {
        // Potentially decrypt it:
        self.server.try_decrypt_modmsg(&mut modmsg).await;

        // If the modmsg is not intented for any of the given
        // channels, ignore it.
        if !self.channels.read().await.contains(&modmsg.channel) {
            return true
        };
        let channels = self.server.channels.read().await;
        let Some(channel) = channels.get(&modmsg.channel) else {
            drop(channels);
            return true
        };

        // Check message signature corresponds to a configured moderator
        // for this channel
        let Ok(signature) = deserialize(&modmsg.signature) else {
            drop(channels);
            return true
        };

        let mut valid = false;
        for moderator in &channel.moderators {
            if moderator.verify(&modmsg.hash(), &signature) {
                valid = true;
                break
            }
        }
        drop(channels);

        if !valid {
            return true
        }

        // Ignore unimplemented commands
        // TODO: add rest commands here and ensure each one is tested
        // TODO: Perhaps this could also be configurable. Like what
        // moderation actions we allow per channel.
        let command = modmsg.command.to_uppercase().to_string();
        if !["MOTD"].contains(&command.as_str()) {
            return true
        }

        // Handle command params lines individually
        for line in modmsg.params.lines() {
            // Skip empty lines
            if line.is_empty() {
                continue
            }

            // Format the message
            let msg = format!("{} {} :{}", command, modmsg.channel, line);

            // Send it to the client
            let reply = ReplyType::Client((String::from("moderator"), msg));
            if let Err(e) = self.reply(writer, &reply).await {
                error!("[IRC CLIENT] Failed writing {} to client: {}", command, e);
                continue
            }
        }

        false
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
