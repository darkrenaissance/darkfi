use async_std::net::TcpStream;
use std::net::SocketAddr;

use futures::{io::WriteHalf, AsyncWriteExt};
use fxhash::FxHashMap;
use log::{debug, info, warn};
use rand::{rngs::OsRng, RngCore};
use ringbuffer::RingBufferWrite;

use darkfi::{net::P2pPtr, system::SubscriberPtr, Error, Result};

use crate::{
    crypto::{encrypt_message, try_decrypt_message},
    privmsg::{Privmsg, PrivmsgsBuffer, SeenMsgIds},
    ChannelInfo,
};

const RPL_NOTOPIC: u32 = 331;
const RPL_TOPIC: u32 = 332;
const RPL_NAMEREPLY: u32 = 353;

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

    pub async fn update(&mut self, line: String) -> Result<()> {
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

                    if self.configured_chans.contains_key(chan) {
                        let chan_info = self.configured_chans.get(chan).unwrap();

                        if chan_info.names.is_empty() {
                            continue
                        }

                        let names_reply = format!(
                            ":{}!anon@dark.fi {} = {} : {}\r\n",
                            self.nickname,
                            RPL_NAMEREPLY,
                            chan,
                            chan_info.names.join(" ")
                        );

                        self.reply(&names_reply).await?;
                    }
                }
            }
            "NICK" => {
                let nickname = tokens.next().ok_or(Error::MalformedPacket)?;
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
                    let join_reply = format!(":{}!anon@dark.fi JOIN {}\r\n", self.nickname, chan);
                    self.reply(&join_reply).await?;
                    if !self.configured_chans.contains_key(chan) {
                        self.configured_chans.insert(chan.to_string(), ChannelInfo::new()?);
                    } else {
                        let chan_info = self.configured_chans.get_mut(chan).unwrap();
                        chan_info.joined = true;
                    }
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
                let channel = tokens.next().ok_or(Error::MalformedPacket)?;
                let substr_idx = line.find(':').ok_or(Error::MalformedPacket)?;

                if substr_idx >= line.len() {
                    return Err(Error::MalformedPacket)
                }

                let message = &line[substr_idx + 1..];
                info!("(Plain) PRIVMSG {} :{}", channel, message);

                if self.configured_chans.contains_key(channel) {
                    let channel_info = self.configured_chans.get(channel).unwrap();
                    if channel_info.joined {
                        let message = if let Some(salt_box) = &channel_info.salt_box {
                            let encrypted = encrypt_message(salt_box, message);
                            info!("(Encrypted) PRIVMSG {} :{}", channel, encrypted);
                            encrypted
                        } else {
                            message.to_string()
                        };

                        let random_id = OsRng.next_u64();

                        let protocol_msg = Privmsg {
                            id: random_id,
                            nickname: self.nickname.clone(),
                            channel: channel.to_string(),
                            message,
                        };

                        {
                            (*self.seen_msg_ids.lock().await).push(random_id);
                            (*self.privmsgs_buffer.lock().await).push(protocol_msg.clone())
                        }

                        self.senders
                            .notify_with_exclude(protocol_msg.clone(), &[self.subscriber_id])
                            .await;

                        debug!(target: "ircd", "PRIVMSG to be sent: {:?}", protocol_msg);
                        self.p2p.broadcast(protocol_msg).await?;
                    }
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

        if !self.is_registered && self.is_nick_init && self.is_user_init {
            debug!("Initializing peer connection");
            let register_reply = format!(":darkfi 001 {} :Let there be dark\r\n", self.nickname);
            self.reply(&register_reply).await?;
            self.is_registered = true;

            // Auto-joins
            macro_rules! autojoin {
                ($channel:expr,$topic:expr) => {
                    let j = format!(":{}!anon@dark.fi JOIN {}\r\n", self.nickname, $channel);
                    let t = format!(":DarkFi TOPIC {} :{}\r\n", $channel, $topic);
                    self.reply(&j).await?;
                    self.reply(&t).await?;
                };
            }

            for chan in self.auto_channels.clone() {
                if self.configured_chans.contains_key(&chan) {
                    let chan_info = self.configured_chans.get_mut(&chan).unwrap();
                    let topic = if let Some(topic) = chan_info.topic.clone() {
                        topic
                    } else {
                        "n/a".to_string()
                    };
                    chan_info.topic = Some(topic.to_string());
                    autojoin!(chan, topic);
                } else {
                    let mut chan_info = ChannelInfo::new()?;
                    chan_info.topic = Some("n/a".to_string());
                    self.configured_chans.insert(chan.clone(), chan_info);
                    autojoin!(chan, "n/a");
                }
            }
        }

        Ok(())
    }

    pub async fn reply(&mut self, message: &str) -> Result<()> {
        self.write_stream.write_all(message.as_bytes()).await?;
        debug!("Sent {}", message);
        Ok(())
    }

    pub async fn process_msg_from_p2p(&mut self, msg: &Privmsg) -> Result<()> {
        let mut msg = msg.clone();
        // Try to potentially decrypt the incoming message.
        if self.configured_chans.contains_key(&msg.channel) {
            let chan_info = self.configured_chans.get_mut(&msg.channel).unwrap();
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
        }

        self.reply(&msg.to_irc_msg()).await?;
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
