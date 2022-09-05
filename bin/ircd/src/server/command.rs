use futures::{AsyncRead, AsyncWrite};
use log::{debug, info, warn};
use ringbuffer::RingBufferWrite;

use darkfi::{Error, Result};

use crate::{crypto::encrypt_privmsg, privmsg::Privmsg, ChannelInfo, MAXIMUM_LENGTH_OF_NICKNAME};

use super::IrcServerConnection;

const RPL_NOTOPIC: u32 = 331;
const RPL_TOPIC: u32 = 332;
const RPL_NAMEREPLY: u32 = 353;
const RPL_ENDOFNAMES: u32 = 366;

impl<C: AsyncRead + AsyncWrite + Send + Unpin + 'static> IrcServerConnection<C> {
    pub(super) fn on_quit(&self) -> Result<()> {
        // Close the connection
        return Err(Error::NetworkServiceStopped)
    }

    pub(super) async fn on_receive_user(&mut self) -> Result<()> {
        // We can stuff any extra things like public keys in here.
        // Ignore it for now.
        if self.is_pass_init {
            self.is_user_init = true;
        } else {
            // Close the connection
            warn!("Password is required");
            return self.on_quit()
        }
        Ok(())
    }

    pub(super) async fn on_receive_pass(&mut self, password: &str) -> Result<()> {
        if &self.password == password {
            self.is_pass_init = true
        } else {
            // Close the connection
            warn!("Password is not correct!");
            return self.on_quit()
        }
        Ok(())
    }

    pub(super) async fn on_receive_nick(&mut self, nickname: &str) -> Result<()> {
        if nickname.len() > MAXIMUM_LENGTH_OF_NICKNAME {
            return Ok(())
        }

        self.is_nick_init = true;
        let old_nick = std::mem::replace(&mut self.nickname, nickname.to_string());

        let nick_reply = format!(":{}!anon@dark.fi NICK {}\r\n", old_nick, self.nickname);
        self.reply(&nick_reply).await
    }

    pub(super) async fn on_receive_part(&mut self, channels: Vec<String>) -> Result<()> {
        for chan in channels.iter() {
            let part_reply = format!(":{}!anon@dark.fi PART {}\r\n", self.nickname, chan);
            self.reply(&part_reply).await?;
            if self.configured_chans.contains_key(chan) {
                let chan_info = self.configured_chans.get_mut(chan).unwrap();
                chan_info.joined = false;
            }
        }
        Ok(())
    }

    pub(super) async fn on_receive_topic(&mut self, line: &str, channel: &str) -> Result<()> {
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
        Ok(())
    }

    pub(super) async fn on_ping(&mut self, value: &str) -> Result<()> {
        let pong = format!("PONG {}\r\n", value);
        self.reply(&pong).await
    }

    pub(super) async fn on_receive_cap(&mut self, line: &str, subcommand: &str) -> Result<()> {
        self.is_cap_end = false;

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

            let cap_ack_reply =
                format!(":{}!anon@dark.fi CAP * ACK :{}\r\n", self.nickname, ack_list.join(" "));

            let cap_nak_reply =
                format!(":{}!anon@dark.fi CAP * NAK :{}\r\n", self.nickname, nak_list.join(" "));

            self.reply(&cap_ack_reply).await?;
            self.reply(&cap_nak_reply).await?;
        }

        if subcommand == "LIST" {
            let enabled_capabilities: Vec<String> =
                self.capabilities.clone().into_iter().filter(|(_, v)| *v).map(|(k, _)| k).collect();

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
        Ok(())
    }

    pub(super) async fn on_receive_names(&mut self, channels: Vec<String>) -> Result<()> {
        for chan in channels.iter() {
            if !chan.starts_with("#") {
                continue
            }
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
        }
        Ok(())
    }

    pub(super) async fn on_receive_privmsg(&mut self, line: &str, target: &str) -> Result<()> {
        let substr_idx = line.find(':').ok_or(Error::MalformedPacket)?;

        if substr_idx >= line.len() {
            return Err(Error::MalformedPacket)
        }

        let message = line[substr_idx + 1..].to_string();

        info!("(Plain) PRIVMSG {} :{}", target, message);

        let mut privmsg = Privmsg::new(self.nickname.clone(), target.to_string(), message, 0);

        if target.starts_with('#') {
            if !self.configured_chans.contains_key(target) {
                return Ok(())
            }

            let channel_info = self.configured_chans.get(target).unwrap();

            if !channel_info.joined {
                return Ok(())
            }

            if let Some(salt_box) = &channel_info.salt_box {
                encrypt_privmsg(salt_box, &mut privmsg);
                info!("(Encrypted) PRIVMSG: {:?}", privmsg);
            }
        } else {
            // If we have a configured secret for this nick, we encrypt the message.
            if let Some(salt_box) = self.configured_contacts.get(target) {
                encrypt_privmsg(salt_box, &mut privmsg);
                info!("(Encrypted) PRIVMSG: {:?}", privmsg);
            }
        }

        {
            (*self.seen_msg_ids.lock().await).push(privmsg.id);
            (*self.privmsgs_buffer.lock().await).push(&privmsg)
        }

        self.senders.notify_with_exclude(privmsg.clone(), &[self.subscriber_id]).await;

        debug!(target: "ircd", "PRIVMSG to be sent: {:?}", privmsg);
        self.p2p.broadcast(privmsg).await?;

        Ok(())
    }

    pub(super) async fn on_receive_join(&mut self, channels: Vec<String>) -> Result<()> {
        for chan in channels.iter() {
            if !chan.starts_with("#") {
                continue
            }
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
                    if msg.target == *chan {
                        self.senders.notify_by_id(msg, self.subscriber_id).await;
                    }
                }
            }
        }
        self.on_receive_names(channels).await?;
        Ok(())
    }
}
