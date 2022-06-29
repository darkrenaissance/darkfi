use async_std::net::TcpStream;
use std::net::SocketAddr;

use futures::{io::WriteHalf, AsyncWriteExt};
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

pub struct IrcServerConnection {
    // server stream
    write_stream: WriteHalf<TcpStream>,
    peer_address: SocketAddr,
    // msg ids
    seen_msg_ids: SeenMsgIds,
    privmsgs_buffer: PrivmsgsBuffer,
    // user & channels
    is_nick_init: bool,
    is_user_init: bool,
    is_registered: bool,
    nickname: String,
    auto_channels: Vec<String>,
    pub configured_chans: FxHashMap<String, ChannelInfo>,
    // p2p
    p2p: P2pPtr,
    senders: SubscriberPtr<Privmsg>,
    subscriber_id: u64,
}

impl IrcServerConnection {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        write_stream: WriteHalf<TcpStream>,
        peer_address: SocketAddr,
        seen_msg_ids: SeenMsgIds,
        privmsgs_buffer: PrivmsgsBuffer,
        auto_channels: Vec<String>,
        configured_chans: FxHashMap<String, ChannelInfo>,
        p2p: P2pPtr,
        senders: SubscriberPtr<Privmsg>,
        subscriber_id: u64,
    ) -> Self {
        Self {
            write_stream,
            peer_address,
            seen_msg_ids,
            privmsgs_buffer,
            is_nick_init: false,
            is_user_init: false,
            is_registered: false,
            nickname: "anon".to_string(),
            auto_channels,
            configured_chans,
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

                if target.starts_with("#") {
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
                }

                self.on_receive_privmsg(&message, target).await?;
            }
            "CAP" => {}
            "QUIT" => {
                // Close the connection
                return Err(Error::NetworkServiceStopped)
            }
            _ => {
                warn!("Unimplemented `{}` command", command);
            }
        }

        // on registration
        if !self.is_registered && self.is_nick_init && self.is_user_init {
            debug!("Initializing peer connection");
            let register_reply = format!(":darkfi 001 {} :Let there be dark\r\n", self.nickname);
            self.reply(&register_reply).await?;
            self.is_registered = true;

            for chan in self.auto_channels.clone() {
                self.on_join(&chan).await?;
            }

            // Send dm messages in buffer
            for msg in self.privmsgs_buffer.lock().await.to_vec() {
                if msg.target == self.nickname || msg.nickname == self.nickname {
                    self.senders.notify_by_id(msg, self.subscriber_id).await;
                }
            }

            // send names command
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
        let topic =
            if let Some(topic) = chan_info.topic.clone() { topic } else { "n/a".to_string() };
        chan_info.topic = Some(topic.to_string());
        chan_info.joined = true;

        {
            let j = format!(":{}!anon@dark.fi JOIN {}\r\n", self.nickname, chan);
            let t = format!(":DarkFi TOPIC {} :{}\r\n", chan, topic);
            self.reply(&j).await?;
            self.reply(&t).await?;
        }

        // Send messages in buffer
        for msg in self.privmsgs_buffer.lock().await.to_vec() {
            if msg.target == chan {
                self.senders.notify_by_id(msg, self.subscriber_id).await;
            }
        }

        self.on_receive_names(chan).await?;
        Ok(())
    }

    pub async fn process_msg_from_p2p(&mut self, msg: &Privmsg) -> Result<()> {
        info!("Received msg from P2p network: {:?}", msg);

        let mut msg = msg.clone();
        // Try to potentially decrypt the incoming message.
        if self.configured_chans.contains_key(&msg.target) {
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
        }

        if self.is_nick_init && (self.nickname == msg.target || self.nickname == msg.nickname) {
            self.reply(&msg.to_irc_msg()).await?;
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

        info!("Send msg to IRC client '{}' from {}", irc_msg, self.peer_address);

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

        Ok(line.clone())
    }
}
