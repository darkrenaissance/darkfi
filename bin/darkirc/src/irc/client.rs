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
    io::Cursor,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst},
        Arc,
    },
};

use darkfi::{
    event_graph::{proto::EventPut, Event, NULL_ID},
    system::Subscription,
    zk::{empty_witnesses, Proof, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_sdk::{
    bridgetree::Position,
    crypto::{pasta_prelude::PrimeField, poseidon_hash, MerkleTree},
    pasta::pallas,
};
use darkfi_serial::{deserialize_async, serialize_async};
use futures::FutureExt;
use sled_overlay::sled;
use smol::{
    io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader},
    lock::{OnceCell, RwLock},
    net::SocketAddr,
    prelude::{AsyncRead, AsyncWrite},
};
use tracing::{debug, error, info, warn};

use super::{
    server::{IrcServer, MAX_MSG_LEN},
    Msg, NickServ, OldPrivmsg, SERVER_NAME,
};
use crate::crypto::rln::{
    closest_epoch, hash_event, RlnIdentity, RLN2_SIGNAL_ZKBIN, RLN_APP_IDENTIFIER,
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
                    // If client closed unexpectedly, we disconnect.
                    if let Ok(0) = r {
                        error!("[IRC CLIENT] Read failed for {}: Client disconnected", self.addr);
                        self.incoming.unsubscribe().await;
                        return Err(Error::ChannelStopped)
                    }
                    // If something failed during reading, we disconnect.
                    if let Err(e) = r {
                        error!("[IRC CLIENT] Read failed for {}: {e}", self.addr);
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

                                // If it fails for some reason, for now, we just note it and pass.
                                if let Err(e) = self.server.darkirc.event_graph.dag_insert(&[event.clone()]).await {
                                    error!("[IRC CLIENT] Failed inserting new event to DAG: {e}");
                                } else {
                                    // We sent this, so it should be considered seen.
                                    if let Err(e) = self.mark_seen(&event_id).await {
                                        error!("[IRC CLIENT] (multiplex_connection) self.mark_seen({event_id}) failed: {e}");
                                        return Err(e)
                                    }

                                    // If we have a RLN identity, now we'll build a ZK proof.
                                    // Also I really want GOTO in Rust... Fags.
                                    if let Some(mut rln_identity) = *self.server.rln_identity.write().await {
                                        // If the current epoch is different, we can reset the message counter
                                        if rln_identity.last_epoch != closest_epoch(event.timestamp) {
                                            rln_identity.last_epoch = closest_epoch(event.timestamp);
                                            rln_identity.message_id = 0;
                                        }

                                        rln_identity.message_id += 1;

                                        let (_proof, _public_inputs) = match self.create_rln_signal_proof(&rln_identity, &event).await {
                                            Ok(v) => v,
                                            Err(e) => {
                                                // TODO: Send a message to the IRC client telling that sending went wrong
                                                error!("[IRC CLIENT] Failed creating RLN signal proof: {e}");
                                                // Just use an empty "proof"
                                                (Proof::new(vec![]), vec![])
                                            }
                                        };

                                        self.server.darkirc.p2p.broadcast(&EventPut(event)).await;
                                    } else {
                                        // Broadcast it
                                        self.server.darkirc.p2p.broadcast(&EventPut(event)).await;
                                    }
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
                //
                // N.b. handling "historical messages", i.e. outstanding messages
                // which have occured when darkirc is offline are handled in
                // <file:./command.rs::async fn get_history(&self, channels: &HashSet<String>) -> Result<Vec<ReplyType>> {>
                // for which the logic for delivery should be kept in sync
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
                            error!("[IRC CLIENT] (multiplex_connection) self.is_seen({event_id}) failed: {e}");
                            return Err(e)
                        }
                    }

                    // If the Event contains an appended blob of data, try to check if it's
                    // a RLN Signal proof and verify it.
                    //if false {
                    let mut verification_failed = false;
                    #[allow(clippy::never_loop)]
                    loop {
                        let (event, blob) = (r.clone(), vec![0,1,2]);
                        let (proof, public_inputs): (Proof, Vec<pallas::Base>) = match deserialize_async(&blob).await {
                            Ok(v) => v,
                            Err(_) => {
                                // TODO: FIXME: This logic should be better written.
                                // Right now we don't enforce RLN so we can just fall-through.
                                //error!("[IRC CLIENT] Failed deserializing event ephemeral data: {e}");
                                break
                            }
                        };

                        if public_inputs.len() != 2 {
                            error!("[IRC CLIENT] Received event has the wrong number of public inputs");
                            verification_failed = true;
                            break
                        }

                        info!("[IRC CLIENT] Verifying incoming Event RLN proof");
                        if self.verify_rln_signal_proof(
                            &event,
                            proof,
                            [public_inputs[0], public_inputs[1]],
                        ).await.is_err() {
                            verification_failed = true;
                            break
                        }

                        // TODO: Store for secret shares recovery
                        info!("[IRC CLIENT] RLN verification successful");
                        break
                    }

                    if verification_failed {
                        error!("[IRC CLIENT] Incoming Event proof verification failed");
                        continue
                    }

                    // Try to deserialize the `Event`'s content into a `Privmsg`
                    let mut privmsg = match Msg::deserialize(r.content()).await {
                        Ok(Msg::V1(old_msg)) => old_msg.into_new(),
                        Ok(Msg::V2(new_msg)) => new_msg,
                        Err(e) => {
                            error!("[IRC CLIENT] Failed deserializing incoming Privmsg event: {e}");
                            continue
                        }
                    };

                    // If successful, potentially decrypt it:
                    self.server.try_decrypt(&mut privmsg, self.nickname.read().await.as_ref()).await;

                    // We should skip any attempts to contact services from the network.
                    if ["nickserv", "chanserv"].contains(&privmsg.nick.to_lowercase().as_str()) {
                        continue
                    }

                    // If the privmsg is not intented for any of the given
                    // channels or contacts, ignore it
                    // otherwise add it as a reply and mark it as seen
                    // in the seen_events tree.
                    let channels = self.channels.read().await;
                    let contacts = self.server.contacts.read().await;
                    if !channels.contains(&privmsg.channel) &&
                        !contacts.contains_key(&privmsg.channel)
                    {
                        continue
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
                        let msg = format!("PRIVMSG {} :{line}", privmsg.channel);

                        // Send it to the client
                        let reply = ReplyType::Client((privmsg.nick.clone(), msg));
                        if let Err(e) = self.reply(&mut writer, &reply).await {
                            error!("[IRC CLIENT] Failed writing PRIVMSG to client: {e}");
                            continue
                        }
                    }

                    // Mark the message as seen for this USER
                    if let Err(e) = self.mark_seen(&event_id).await {
                        error!("[IRC CLIENT] (multiplex_connection) self.mark_seen({event_id}) failed: {e}");
                        return Err(e)
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
            ReplyType::Server((rpl, msg)) => format!(":{SERVER_NAME} {rpl:03} {msg}"),
            ReplyType::Client((nick, msg)) => format!(":{nick}!~anon@darkirc {msg}"),
            ReplyType::Pong(origin) => format!(":{SERVER_NAME} PONG :{origin}"),
            ReplyType::Cap(msg) => format!(":{SERVER_NAME} {msg}"),
            ReplyType::Notice((src, dst, msg)) => {
                format!(":{src}!~anon@darkirc NOTICE {dst} :{msg}")
            }
        };

        debug!("[{}] <-- {r}", self.addr);

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
        args_queue: &mut VecDeque<OldPrivmsg>,
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
        if line.ends_with("\r\n") {
            line.pop();
            line.pop();
        } else if line.ends_with("\n") {
            line.pop();
        } else {
            return Err(Error::ParseFailed("Line doesn't end with CR/LF"))
        }

        // Prefix the message part of PRIVMSG with ':' if is not already.
        // Or realname part of USER command.
        if let Some(index) = match line.split_whitespace().next() {
            Some("PRIVMSG") => Some(2),
            Some("USER") => Some(4),
            _ => None,
        } {
            let mut words: Vec<String> =
                line.splitn(index + 1, char::is_whitespace).map(|s| s.to_string()).collect();
            if words.len() > index && !words[index].starts_with(':') {
                words[index] = format!(":{}", words[index]);
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

        debug!("[{}] --> {cmd}{args}", self.addr);

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
                warn!("[IRC CLIENT] Unimplemented \"{cmd}\" command");
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
            // If the DAG is not synced yet, queue client lines
            // Once synced, send queued lines and continue as normal
            if !*self.server.darkirc.event_graph.synced.read().await {
                debug!("DAG is still syncing, queuing and skipping...");
                let privmsg = self.args_to_privmsg(args).await;
                args_queue.push_back(privmsg);
                return Ok(None)
            }

            // Check if we have queued PRIVMSGs, if we do send all of them first.
            let mut pending_events = vec![];
            if !args_queue.is_empty() {
                for _ in 0..args_queue.len() {
                    let privmsg = args_queue.pop_front().unwrap();
                    pending_events.push(self.privmsg_to_event(privmsg).await);
                }
                return Ok(Some(pending_events))
            }

            // If queue is empty, create an event and return it
            let privmsg = self.args_to_privmsg(args).await;
            let event = self.privmsg_to_event(privmsg).await;

            return Ok(Some(vec![event]))
        }

        Ok(None)
    }

    // Internal helper function that creates a PRIVMSG from IRC client arguments
    async fn args_to_privmsg(&self, args: String) -> OldPrivmsg {
        let nick = self.nickname.read().await.to_string();
        let channel = args.split_ascii_whitespace().next().unwrap().to_string();
        let msg_offset = args.find(':').unwrap() + 1;
        let (_, msg) = args.split_at(msg_offset);

        // Truncate messages longer than MAX_MSG_LEN
        let msg = if msg.len() > MAX_MSG_LEN { msg.split_at(MAX_MSG_LEN).0 } else { msg };
        OldPrivmsg { channel, nick, msg: msg.to_string() }
    }

    // Internal helper function that creates an Event from PRIVMSG arguments
    async fn privmsg_to_event(&self, mut privmsg: OldPrivmsg) -> Event {
        // Encrypt the Privmsg if an encryption method is available.
        self.server.try_encrypt(&mut privmsg).await;

        // Build a DAG event and return it.
        Event::new(serialize_async(&privmsg).await, &self.server.darkirc.event_graph).await
    }

    /// Atomically mark a message as seen for this client.
    pub async fn mark_seen(&self, event_id: &blake3::Hash) -> Result<()> {
        let db = self
            .seen
            .get_or_init(|| async {
                let u = self.username.read().await.to_string();
                self.server.darkirc.sled.open_tree(format!("darkirc_user_{u}")).unwrap()
            })
            .await;

        debug!("Marking event {event_id} as seen");
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
                self.server.darkirc.sled.open_tree(format!("darkirc_user_{u}")).unwrap()
            })
            .await;

        Ok(db.contains_key(event_id.as_bytes())?)
    }

    /// Abstraction for RLN signal proof creation
    async fn create_rln_signal_proof(
        &self,
        rln_identity: &RlnIdentity,
        event: &Event,
    ) -> Result<(Proof, Vec<pallas::Base>)> {
        let identity_commitment = rln_identity.commitment();

        // Fetch the commitment's leaf position in the Merkle tree
        let Some(identity_pos) =
            self.server.rln_identity_store.get(identity_commitment.to_repr())?
        else {
            return Err(Error::DatabaseError(
                "Identity not found in commitment tree store".to_string(),
            ))
        };
        let identity_pos: Position = deserialize_async(&identity_pos).await?;

        // Fetch the latest commitment Merkle tree
        let Some(identity_tree) = self.server.server_store.get("rln_identity_tree")? else {
            return Err(Error::DatabaseError(
                "RLN Identity tree not found in server store".to_string(),
            ))
        };
        let identity_tree: MerkleTree = deserialize_async(&identity_tree).await?;

        // Retrieve the ZK proving key from the db
        let signal_zkbin = ZkBinary::decode(RLN2_SIGNAL_ZKBIN)?;
        let signal_circuit = ZkCircuit::new(empty_witnesses(&signal_zkbin)?, &signal_zkbin);
        let Some(proving_key) = self.server.server_store.get("rlnv2-diff-signal-pk")? else {
            return Err(Error::DatabaseError(
                "RLN signal proving key not found in server store".to_string(),
            ))
        };
        let mut reader = Cursor::new(proving_key);
        let proving_key = ProvingKey::read(&mut reader, signal_circuit)?;

        rln_identity.create_signal_proof(event, &identity_tree, identity_pos, &proving_key)
    }

    /// Abstraction for RLN signal proof verification
    async fn verify_rln_signal_proof(
        &self,
        event: &Event,
        proof: Proof,
        public_inputs: [pallas::Base; 2],
    ) -> Result<()> {
        let epoch = pallas::Base::from(closest_epoch(event.timestamp));
        let external_nullifier = poseidon_hash([epoch, RLN_APP_IDENTIFIER]);
        let x = hash_event(event);
        let y = public_inputs[0];
        let internal_nullifier = public_inputs[1];

        // Fetch the latest commitment Merkle tree
        let Some(identity_tree) = self.server.server_store.get("rln_identity_tree")? else {
            return Err(Error::DatabaseError(
                "RLN Identity tree not found in server store".to_string(),
            ))
        };
        let identity_tree: MerkleTree = deserialize_async(&identity_tree).await?;
        let identity_root = identity_tree.root(0).unwrap();

        let public_inputs =
            vec![epoch, external_nullifier, x, y, internal_nullifier, identity_root.inner()];

        Ok(proof.verify(&self.server.rln_signal_vk, &public_inputs)?)
    }
}
