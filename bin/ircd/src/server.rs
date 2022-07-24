use std::net::SocketAddr;

use futures::{io::WriteHalf, AsyncRead, AsyncWrite, AsyncWriteExt};
use fxhash::FxHashMap;
use log::{debug, info, warn};
use rand::{rngs::OsRng, RngCore};
use ringbuffer::{RingBufferExt, RingBufferWrite};

use darkfi::{net::P2pPtr, system::SubscriberPtr, Error, Result};

use crate::{
    crypto::{encrypt_message, try_decrypt_message},
    privmsg::{Privmsg, PrivmsgsBuffer, SeenMsgIds},
    ChannelInfo, MAXIMUM_LENGTH_OF_MESSAGE, MAXIMUM_LENGTH_OF_NICKNAME,
};

const RPL_NOTOPIC: u32 = 331;
const RPL_TOPIC: u32 = 332;
const RPL_NAMEREPLY: u32 = 353;
const RPL_ENDOFNAMES: u32 = 366;

pub struct IrcServerConnection<C: AsyncRead + AsyncWrite + Send + Unpin + 'static> {
    // server stream
    write_stream: WriteHalf<C>,
    peer_address: SocketAddr,
    // msg ids
    seen_msg_ids: SeenMsgIds,
    privmsgs_buffer: PrivmsgsBuffer,
    // user & channels
    is_nick_init: bool,
    is_user_init: bool,
    is_registered: bool,
    is_cap_end: bool,
    nickname: String,
    auto_channels: Vec<String>,
    pub configured_chans: FxHashMap<String, ChannelInfo>,
    pub configured_contacts: FxHashMap<String, crypto_box::Box>,
    capabilities: FxHashMap<String, bool>,
    // p2p
    p2p: P2pPtr,
    senders: SubscriberPtr<Privmsg>,
    subscriber_id: u64,
}

impl<C: AsyncRead + AsyncWrite + Send + Unpin + 'static> IrcServerConnection<C> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        write_stream: WriteHalf<C>,
        peer_address: SocketAddr,
        seen_msg_ids: SeenMsgIds,
        privmsgs_buffer: PrivmsgsBuffer,
        auto_channels: Vec<String>,
        configured_chans: FxHashMap<String, ChannelInfo>,
        configured_contacts: FxHashMap<String, crypto_box::Box>,
        p2p: P2pPtr,
        senders: SubscriberPtr<Privmsg>,
        subscriber_id: u64,
    ) -> Self {
        let mut capabilities = FxHashMap::default();
        capabilities.insert("no-history".to_string(), false);
        Self {
            write_stream,
            peer_address,
            seen_msg_ids,
            privmsgs_buffer,
            is_nick_init: false,
            is_user_init: false,
            is_registered: false,
            is_cap_end: true,
            nickname: "anon".to_string(),
            auto_channels,
            configured_chans,
            configured_contacts,
            capabilities,
            p2p,
            senders,
            subscriber_id,
        }
    }

    async fn update(&mut self, line: String) -> Result<()> {
        if line.len() > MAXIMUM_LENGTH_OF_MESSAGE {
            return Err(Error::MalformedPacket)
        }

        let mut tokens = line.split_ascii_whitespace();
        // Commands can begin with :garbage but we will reject clients doing
        // that for now to keep the protocol simple and focused.
        let command = tokens.next().ok_or(Error::MalformedPacket)?;

        info!("IRC server received command: {}", command.to_uppercase());

        match command.to_uppercase().as_str() {
            "USER" => {
                // We can stuff any extra things like public keys in here.
                // Ignore it for now.
                self.is_user_init = true;
            }
            "NAMES" => {
                let channels = tokens.next().ok_or(Error::MalformedPacket)?;
                for chan in channels.split(',') {
                    if !chan.starts_with('#') {
                        warn!("{} is not a valid name for channel", chan);
                        continue
                    }

                    self.on_receive_names(chan).await?;
                }
            }
            "NICK" => {
                let nickname = tokens.next().ok_or(Error::MalformedPacket)?;

                if nickname.len() > MAXIMUM_LENGTH_OF_NICKNAME {
                    return Ok(())
                }

                self.is_nick_init = true;
                let old_nick = std::mem::replace(&mut self.nickname, nickname.to_string());

                let nick_reply = format!(":{}!anon@dark.fi NICK {}\r\n", old_nick, self.nickname);
                self.reply(&nick_reply).await?;
            }
            "JOIN" => {
                let channels = tokens.next().ok_or(Error::MalformedPacket)?;
                for chan in channels.split(',') {
                    if !chan.starts_with('#') {
                        warn!("{} is not a valid name for channel", chan);
                        continue
                    }

                    self.on_join(chan).await?;
                }
            }
            "PART" => {
                let channels = tokens.next().ok_or(Error::MalformedPacket)?;
                for chan in channels.split(',') {
                    let part_reply = format!(":{}!anon@dark.fi PART {}\r\n", self.nickname, chan);
                    self.reply(&part_reply).await?;
                    if self.configured_chans.contains_key(chan) {
                        let chan_info = self.configured_chans.get_mut(chan).unwrap();
                        chan_info.joined = false;
                    }
                }
            }
            "TOPIC" => {
                let channel = tokens.next().ok_or(Error::MalformedPacket)?;
                if let Some(substr_idx) = line.find(':') {
                    // Client is setting the topic
                    if substr_idx >= line.len() {
                        return Err(Error::MalformedPacket)
                    }

                    let topic = &line[substr_idx + 1..];
                    let chan_info = self.configured_chans.get_mut(channel).unwrap();
                    chan_info.topic = Some(topic.to_string());

                    let topic_reply =
                        format!(":{}!anon@dark.fi TOPIC {} :{}\r\n", self.nickname, channel, topic);
                    self.reply(&topic_reply).await?;
                } else {
                    // Client is asking or the topic
                    let chan_info = self.configured_chans.get(channel).unwrap();
                    let topic_reply = if let Some(topic) = &chan_info.topic {
                        format!("{} {} {} :{}\r\n", RPL_TOPIC, self.nickname, channel, topic)
                    } else {
                        const TOPIC: &str = "No topic is set";
                        format!("{} {} {} :{}\r\n", RPL_NOTOPIC, self.nickname, channel, TOPIC)
                    };
                    self.reply(&topic_reply).await?;
                }
            }
            "PING" => {
                let pong = tokens.next().ok_or(Error::MalformedPacket)?;
                let pong = format!("PONG {}\r\n", pong);
                self.reply(&pong).await?;
            }
            "PRIVMSG" => {
                let target = tokens.next().ok_or(Error::MalformedPacket)?;
                let substr_idx = line.find(':').ok_or(Error::MalformedPacket)?;

                if substr_idx >= line.len() {
                    return Err(Error::MalformedPacket)
                }

                let mut message = line[substr_idx + 1..].to_string();
                info!("(Plain) PRIVMSG {} :{}", target, message);

                if target.starts_with('#') {
                    if !self.configured_chans.contains_key(target) {
                        return Ok(())
                    }

                    let channel_info = self.configured_chans.get(target).unwrap();

                    if !channel_info.joined {
                        return Ok(())
                    }

                    message = if let Some(salt_box) = &channel_info.salt_box {
                        let encrypted = encrypt_message(salt_box, &message);
                        info!("(Encrypted) PRIVMSG {} :{}", target, encrypted);
                        encrypted
                    } else {
                        message.to_string()
                    };
                } else {
                    // If we have a configured secret for this nick, we encrypt the message.
                    if let Some(salt_box) = self.configured_contacts.get(target) {
                        message = encrypt_message(salt_box, &message);
                        info!("(Encrypted) PRIVMSG {} :{}", target, message);
                    }
                }

                self.on_receive_privmsg(&message, target).await?;
            }
            "CAP" => {
                self.is_cap_end = false;

                let subcommand = tokens.next().ok_or(Error::MalformedPacket)?.to_uppercase();

                let capabilities_keys: Vec<String> = self.capabilities.keys().cloned().collect();

                if subcommand == "LS" {
                    let cap_ls_reply = format!(
                        ":{}!anon@dark.fi CAP * LS :{}\r\n",
                        self.nickname,
                        capabilities_keys.join(" ")
                    );
                    self.reply(&cap_ls_reply).await?;
                }

                if subcommand == "REQ" {
                    let substr_idx = line.find(':').ok_or(Error::MalformedPacket)?;

                    if substr_idx >= line.len() {
                        return Err(Error::MalformedPacket)
                    }

                    let cap: Vec<&str> = line[substr_idx + 1..].split(' ').collect();

                    let mut ack_list = vec![];
                    let mut nak_list = vec![];

                    for c in cap {
                        if self.capabilities.contains_key(c) {
                            self.capabilities.insert(c.to_string(), true);
                            ack_list.push(c);
                        } else {
                            nak_list.push(c);
                        }
                    }

                    let cap_ack_reply = format!(
                        ":{}!anon@dark.fi CAP * ACK :{}\r\n",
                        self.nickname,
                        ack_list.join(" ")
                    );

                    let cap_nak_reply = format!(
                        ":{}!anon@dark.fi CAP * NAK :{}\r\n",
                        self.nickname,
                        nak_list.join(" ")
                    );

                    self.reply(&cap_ack_reply).await?;
                    self.reply(&cap_nak_reply).await?;
                }

                if subcommand == "LIST" {
                    let enabled_capabilities: Vec<String> = self
                        .capabilities
                        .clone()
                        .into_iter()
                        .filter(|(_, v)| *v)
                        .map(|(k, _)| k)
                        .collect();

                    let cap_list_reply = format!(
                        ":{}!anon@dark.fi CAP * LIST :{}\r\n",
                        self.nickname,
                        enabled_capabilities.join(" ")
                    );
                    self.reply(&cap_list_reply).await?;
                }

                if subcommand == "END" {
                    self.is_cap_end = true;
                }
            }
            "QUIT" => {
                // Close the connection
                return Err(Error::NetworkServiceStopped)
            }
            _ => {
                warn!("Unimplemented `{}` command", command);
            }
        }

        // on registration
        if !self.is_registered && self.is_cap_end && self.is_nick_init && self.is_user_init {
            debug!("Initializing peer connection");
            let register_reply = format!(":darkfi 001 {} :Let there be dark\r\n", self.nickname);
            self.reply(&register_reply).await?;
            self.is_registered = true;

            for chan in self.auto_channels.clone() {
                self.on_join(&chan).await?;
            }

            // Send dm messages in buffer
            if *self.capabilities.get("no-history").unwrap() {
                return Ok(())
            }

            for msg in self.privmsgs_buffer.lock().await.to_vec() {
                if msg.target == self.nickname ||
                    (msg.nickname == self.nickname && !msg.target.starts_with('#'))
                {
                    self.senders.notify_by_id(msg, self.subscriber_id).await;
                }
            }
        }

        Ok(())
    }

    async fn reply(&mut self, message: &str) -> Result<()> {
        self.write_stream.write_all(message.as_bytes()).await?;
        debug!("Sent {}", message);
        Ok(())
    }

    async fn on_receive_names(&mut self, chan: &str) -> Result<()> {
        if self.configured_chans.contains_key(chan) {
            let chan_info = self.configured_chans.get(chan).unwrap();

            if chan_info.names.is_empty() {
                return Ok(())
            }

            let names_reply = format!(
                ":{}!anon@dark.fi {} = {} : {}\r\n",
                self.nickname,
                RPL_NAMEREPLY,
                chan,
                chan_info.names.join(" ")
            );

            self.reply(&names_reply).await?;

            let end_of_names = format!(
                ":DarkFi {:03} {} {} :End of NAMES list\r\n",
                RPL_ENDOFNAMES, self.nickname, chan
            );

            self.reply(&end_of_names).await?;
        }

        Ok(())
    }

    async fn on_receive_privmsg(&mut self, message: &str, target: &str) -> Result<()> {
        let random_id = OsRng.next_u64();

        let protocol_msg = Privmsg {
            id: random_id,
            nickname: self.nickname.clone(),
            target: target.to_string(),
            message: message.to_string(),
        };

        {
            (*self.seen_msg_ids.lock().await).push(random_id);
            (*self.privmsgs_buffer.lock().await).push(protocol_msg.clone())
        }

        self.senders.notify_with_exclude(protocol_msg.clone(), &[self.subscriber_id]).await;

        debug!(target: "ircd", "PRIVMSG to be sent: {:?}", protocol_msg);
        self.p2p.broadcast(protocol_msg).await?;

        Ok(())
    }

    async fn on_join(&mut self, chan: &str) -> Result<()> {
        if !self.configured_chans.contains_key(chan) {
            let mut chan_info = ChannelInfo::new()?;
            chan_info.topic = Some("n/a".to_string());
            self.configured_chans.insert(chan.to_string(), chan_info);
        }

        let chan_info = self.configured_chans.get_mut(chan).unwrap();
        if chan_info.joined {
            return Ok(())
        }
        chan_info.joined = true;

        let topic =
            if let Some(topic) = chan_info.topic.clone() { topic } else { "n/a".to_string() };
        chan_info.topic = Some(topic.to_string());

        {
            let j = format!(":{}!anon@dark.fi JOIN {}\r\n", self.nickname, chan);
            let t = format!(":DarkFi TOPIC {} :{}\r\n", chan, topic);
            self.reply(&j).await?;
            self.reply(&t).await?;
        }

        // Send messages in buffer
        if !self.capabilities.get("no-history").unwrap() {
            for msg in self.privmsgs_buffer.lock().await.to_vec() {
                if msg.target == chan {
                    self.senders.notify_by_id(msg, self.subscriber_id).await;
                }
            }
        }

        self.on_receive_names(chan).await?;
        Ok(())
    }

    pub async fn process_msg_from_p2p(&mut self, msg: &Privmsg) -> Result<()> {
        info!("Received msg from P2p network: {:?}", msg);

        let mut msg = msg.clone();

        if msg.target.starts_with('#') {
            // Try to potentially decrypt the incoming message.
            if !self.configured_chans.contains_key(&msg.target) {
                return Ok(())
            }

            let chan_info = self.configured_chans.get_mut(&msg.target).unwrap();
            if !chan_info.joined {
                return Ok(())
            }

            let salt_box = chan_info.salt_box.clone();

            if salt_box.is_some() {
                let decrypted_msg = try_decrypt_message(&salt_box.unwrap(), &msg.message);

                if decrypted_msg.is_none() {
                    return Ok(())
                }

                msg.message = decrypted_msg.unwrap();
                info!("Decrypted received message: {:?}", msg);
            }

            // add the nickname to the channel's names
            if !chan_info.names.contains(&msg.nickname) {
                chan_info.names.push(msg.nickname.clone());
            }

            self.reply(&msg.to_irc_msg()).await?;
            return Ok(())
        } else {
            if self.is_cap_end &&
                self.is_nick_init &&
                (self.nickname == msg.target || self.nickname == msg.nickname)
            {
                if self.configured_contacts.contains_key(&msg.target) {
                    let salt_box = self.configured_contacts.get(&msg.target).unwrap();
                    if let Some(decrypted) = try_decrypt_message(&salt_box, &msg.message) {
                        msg.message = decrypted;
                        info!("Decrypted received message: {:?}", msg);
                    }
                }

                self.reply(&msg.to_irc_msg()).await?;
            }
        }

        Ok(())
    }

    pub async fn process_line_from_client(
        &mut self,
        err: std::result::Result<usize, std::io::Error>,
        line: String,
    ) -> Result<()> {
        if let Err(e) = err {
            warn!("Read line error {}: {}", self.peer_address, e);
            return Err(Error::ChannelStopped)
        }

        info!("Received msg from IRC client: {:?}", line);
        let irc_msg = self.clean_input_line(line)?;

        if let Err(e) = self.update(irc_msg).await {
            warn!("Connection error: {} for {}", e, self.peer_address);
            return Err(Error::ChannelStopped)
        }
        Ok(())
    }

    fn clean_input_line(&self, mut line: String) -> Result<String> {
        if line.is_empty() {
            warn!("Received empty line from {}. ", self.peer_address);
            warn!("Closing connection.");
            return Err(Error::ChannelStopped)
        }

        if &line[(line.len() - 2)..] == "\r\n" {
            // Remove CRLF
            line.pop();
            line.pop();
        } else if &line[(line.len() - 1)..] == "\n" {
            line.pop();
        } else {
            warn!("Closing connection.");
            return Err(Error::ChannelStopped)
        }

        if line == "\n" {
            warn!("Closing connection.");
            return Err(Error::ChannelStopped)
        }

        Ok(line.clone())
    }
}
