use std::str::FromStr;

use async_std::net::TcpStream;
use futures::{io::WriteHalf, AsyncWriteExt};
use fxhash::FxHashMap;
use log::{debug, info, warn};
use rand::{rngs::OsRng, RngCore};

use darkfi::{Error, Result};

use crate::{crypto::encrypt_message, privmsg::Privmsg, ChannelInfo, SeenMsgIds};

const RPL_NOTOPIC: u32 = 331;
const RPL_TOPIC: u32 = 332;

pub struct IrcServerConnection {
    write_stream: WriteHalf<TcpStream>,
    is_nick_init: bool,
    is_user_init: bool,
    is_registered: bool,
    nickname: String,
    seen_msg_id: SeenMsgIds,
    p2p_sender: async_channel::Sender<Privmsg>,
    auto_channels: Vec<String>,
    pub configured_chans: FxHashMap<String, ChannelInfo>,
}

impl IrcServerConnection {
    pub fn new(
        write_stream: WriteHalf<TcpStream>,
        seen_msg_id: SeenMsgIds,
        p2p_sender: async_channel::Sender<Privmsg>,
        auto_channels: Vec<String>,
        configured_chans: FxHashMap<String, ChannelInfo>,
    ) -> Self {
        Self {
            write_stream,
            is_nick_init: false,
            is_user_init: false,
            is_registered: false,
            nickname: "anon".to_string(),
            seen_msg_id,
            p2p_sender,
            auto_channels,
            configured_chans,
        }
    }

    pub async fn update(&mut self, line: String) -> Result<()> {
        let mut tokens = line.split_ascii_whitespace();
        // Commands can begin with :garbage but we will reject clients doing
        // that for now to keep the protocol simple and focused.
        let command = tokens.next().ok_or(Error::MalformedPacket)?;

        info!("Received command: {}", command.to_uppercase());

        match command.to_uppercase().as_str() {
            "USER" => {
                // We can stuff any extra things like public keys in here.
                // Ignore it for now.
                self.is_user_init = true;
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
                    let join_reply = format!(":{}!anon@dark.fi JOIN {}\r\n", self.nickname, chan);
                    self.reply(&join_reply).await?;
                    if !self.configured_chans.contains_key(chan) {
                        self.configured_chans.insert(chan.to_string(), ChannelInfo::new()?);
                    }
                }
            }
            "PART" => {
                let channels = tokens.next().ok_or(Error::MalformedPacket)?;
                for chan in channels.split(',') {
                    let part_reply = format!(":{}!anon@dark.fi PART {}\r\n", self.nickname, chan);
                    self.reply(&part_reply).await?;
                    let chan_info = self.configured_chans.get_mut(chan).unwrap();
                    chan_info.joined = false;
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
                let line_clone = line.clone();
                let split_line: Vec<&str> = line_clone.split_whitespace().collect();
                if split_line.len() > 1 && split_line[0] == "PING" {
                    let pong = format!("PONG {}\r\n", split_line[1]);
                    self.reply(&pong).await?;
                }
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

                        let random_id = OsRng.next_u32();

                        let protocol_msg = Privmsg {
                            id: random_id,
                            nickname: self.nickname.clone(),
                            channel: channel.to_string(),
                            message,
                        };

                        let mut smi = self.seen_msg_id.lock().await;
                        smi.push(random_id);
                        drop(smi);

                        debug!(target: "ircd", "PRIVMSG to be sent: {:?}", protocol_msg);
                        self.p2p_sender.send(protocol_msg).await?;
                    }
                }
            }
            "QUIT" => {
                // Close the connection
                return Err(Error::ServiceStopped)
            }
            // Below, we implement custom server commands that do not conform
            // to the IRC specification. These are specific to our implementation.
            "MSGHIST" => {
                // Fetch the message history for a certain channel with optional
                // max limit.
                // MSGHIST #channel num_msgs
                let channel = tokens.next().ok_or(Error::MalformedPacket)?;
                let num_msgs = if let Some(n) = tokens.next() { i64::from_str(n)? } else { -1 };
                info!("Fetching last {} messages for {}", num_msgs, channel);

                if num_msgs < 0 {
                    // Fetch all messages for the channel
                } else {
                    // Fetch newest num_msgs for the channel
                }
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
}
