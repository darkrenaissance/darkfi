/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use std::net::SocketAddr;

use async_std::sync::{Arc, Mutex};
use futures::{
    io::{BufReader, ReadHalf, WriteHalf},
    AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, FutureExt,
};

use log::{debug, error, info, warn};

use darkfi::{event_graph::model::Event, system::Subscription, Error, Result};

use crate::{
    crypto::{decrypt_privmsg, decrypt_target, encrypt_privmsg},
    settings,
    settings::RPL,
    ChannelInfo, PrivMsgEvent,
};

use super::{ClientSubMsg, IrcConfig, NotifierMsg};

pub struct IrcClient<C: AsyncRead + AsyncWrite + Send + Unpin + 'static> {
    // network stream
    write_stream: WriteHalf<C>,
    read_stream: BufReader<ReadHalf<C>>,
    pub address: SocketAddr,

    // irc config
    irc_config: IrcConfig,

    server_notifier: smol::channel::Sender<(NotifierMsg, u64)>,
    subscription: Subscription<ClientSubMsg>,

    missed_events: Arc<Mutex<Vec<Event<PrivMsgEvent>>>>,
}

impl<C: AsyncRead + AsyncWrite + Send + Unpin + 'static> IrcClient<C> {
    pub fn new(
        write_stream: WriteHalf<C>,
        read_stream: BufReader<ReadHalf<C>>,
        address: SocketAddr,
        irc_config: IrcConfig,
        server_notifier: smol::channel::Sender<(NotifierMsg, u64)>,
        subscription: Subscription<ClientSubMsg>,
        missed_events: Arc<Mutex<Vec<Event<PrivMsgEvent>>>>,
    ) -> Self {
        Self {
            write_stream,
            read_stream,
            address,
            irc_config,
            subscription,
            server_notifier,
            missed_events,
        }
    }

    /// Start listening for messages came from View or irc client
    pub async fn listen(&mut self) {
        loop {
            let mut line = String::new();

            futures::select! {
                // Process msg from View or other client connnected to the same irc server
                msg = self.subscription.receive().fuse() => {
                    match msg {
                        ClientSubMsg::Privmsg(mut m) => {
                            if let Err(e) = self.process_msg(&mut m).await {
                                error!("[CLIENT {}] Process msg: {}",  self.address, e);
                                break
                            }
                        }
                        ClientSubMsg::Config(c) => self.update_config(c).await,
                    }

                }
                // Process msg from IRC client
                err = self.read_stream.read_line(&mut line).fuse() => {
                    if let Err(e) = err {
                        error!("[CLIENT {}] Read line error: {}", self.address, e);
                        break
                    }
                    if let Err(e) = self.process_line(line).await {
                        error!("[CLIENT {}] Process line failed: {}",  self.address, e);
                        break
                    }
                }
            }
        }

        warn!("[CLIENT {}] Close connection", self.address);
        self.subscription.unsubscribe().await;
    }

    pub async fn update_config(&mut self, new_config: IrcConfig) {
        info!("[CLIENT {}] Updating config...", self.address);

        self.irc_config.channels.extend(new_config.channels);
        self.irc_config.contacts.extend(new_config.contacts);
        self.irc_config.nickname = new_config.nickname;
        self.irc_config.private_key = new_config.private_key;
        self.irc_config.password = new_config.password;

        if self.on_receive_join(self.irc_config.channels.keys().cloned().collect()).await.is_err() {
            warn!("Error to join updated channels");
        }
    }

    pub async fn process_msg(&mut self, msg: &mut PrivMsgEvent) -> Result<()> {
        info!("[CLIENT {}] msg from View: {:?}", self.address, msg.to_string());

        decrypt_target(
            msg,
            &self.irc_config.channels,
            &self.irc_config.contacts,
            &self.irc_config.private_key,
        );

        if msg.target.starts_with('#') {
            // Try to potentially decrypt the incoming message.
            if !self.irc_config.channels.contains_key(&msg.target) {
                return Ok(())
            }

            let chan_info = self.irc_config.channels.get_mut(&msg.target).unwrap();
            if !chan_info.joined {
                return Ok(())
            }

            if let Some(salt_box) = &chan_info.salt_box(&msg.target) {
                decrypt_privmsg(salt_box, msg);
                info!(
                    "[CLIENT {}] Decrypted received message: {:?}",
                    self.address,
                    msg.to_string()
                );
            }

            // add the nickname to the channel's names
            if !chan_info.names.contains(&msg.nick) {
                chan_info.names.push(msg.nick.clone());
            }

            self.reply(&msg.to_string()).await?;
        } else if self.irc_config.private_key.is_some() {
            if let Some(contact_info) = self.irc_config.contacts.get(&msg.target) {
                let salt_box = &contact_info
                    .salt_box(self.irc_config.private_key.as_ref().unwrap(), &msg.target);

                if salt_box.is_none() {
                    return Ok(())
                }

                decrypt_privmsg(salt_box.as_ref().unwrap(), msg);

                info!(
                    "[CLIENT {}] Decrypted received message: {:?}",
                    self.address,
                    msg.to_string()
                );

                self.reply(&msg.to_string()).await?;
            }
        }

        Ok(())
    }

    pub async fn process_line(&mut self, line: String) -> Result<()> {
        let irc_msg = match clean_input_line(line) {
            Ok(msg) => msg,
            Err(e) => {
                warn!("[CLIENT {}] Connection error: {}", self.address, e);
                return Err(Error::ChannelStopped)
            }
        };

        info!("[CLIENT {}] Process msg: {}", self.address, irc_msg);

        if let Err(e) = self.update(irc_msg).await {
            warn!("[CLIENT {}] Connection error: {}", self.address, e);
            return Err(Error::ChannelStopped)
        }
        Ok(())
    }

    async fn update(&mut self, line: String) -> Result<()> {
        if line.len() > settings::MAXIMUM_LENGTH_OF_MESSAGE {
            return Err(Error::MalformedPacket)
        }

        let (command, value) = parse_line(&line)?;
        let (command, value) = (command.as_str(), value.as_str());

        match command {
            "PASS" => self.on_receive_pass(value).await?,
            "USER" => self.on_receive_user().await?,
            "NAMES" => self.on_receive_names(value.split(',').map(String::from).collect()).await?,
            "NICK" => self.on_receive_nick(value).await?,
            "JOIN" => self.on_receive_join(value.split(',').map(String::from).collect()).await?,
            "PART" => self.on_receive_part(value.split(',').map(String::from).collect()).await?,
            "TOPIC" => self.on_receive_topic(&line, value).await?,
            "PING" => self.on_ping(value).await?,
            "PRIVMSG" => self.on_receive_privmsg(&line, value).await?,
            "CAP" => self.on_receive_cap(&line, &value.to_uppercase()).await?,
            "QUIT" => self.on_quit()?,
            _ => warn!("[CLIENT {}] Unimplemented `{}` command", self.address, command),
        }

        self.registre().await?;
        Ok(())
    }

    async fn registre(&mut self) -> Result<()> {
        if !self.irc_config.is_pass_init && self.irc_config.password.is_empty() {
            self.irc_config.is_pass_init = true
        }

        if !self.irc_config.is_registered &&
            self.irc_config.is_cap_end &&
            self.irc_config.is_nick_init &&
            self.irc_config.is_user_init
        {
            debug!("Initializing peer connection");
            let register_reply =
                format!(":darkfi 001 {} :Let there be dark\r\n", self.irc_config.nickname);
            self.reply(&register_reply).await?;
            self.irc_config.is_registered = true;

            // join all channels
            self.on_receive_join(self.irc_config.channels.keys().cloned().collect()).await?;

            if *self.irc_config.capabilities.get("no-history").unwrap() {
                return Ok(())
            }
        }
        Ok(())
    }

    async fn reply(&mut self, message: &str) -> Result<()> {
        self.write_stream.write_all(message.as_bytes()).await?;
        debug!("Sent {}", message.trim_end());
        Ok(())
    }

    fn on_quit(&self) -> Result<()> {
        // Close the connection
        Err(Error::NetworkServiceStopped)
    }

    async fn on_receive_user(&mut self) -> Result<()> {
        // We can stuff any extra things like public keys in here.
        // Ignore it for now.
        if self.irc_config.is_pass_init {
            self.irc_config.is_user_init = true;
        } else {
            // Close the connection
            warn!("[CLIENT {}] Password is required", self.address);
            return self.on_quit()
        }
        Ok(())
    }

    async fn on_receive_pass(&mut self, password: &str) -> Result<()> {
        if self.irc_config.password == password {
            self.irc_config.is_pass_init = true
        } else {
            // Close the connection
            warn!("[CLIENT {}] Password is not correct!", self.address);
            return self.on_quit()
        }
        Ok(())
    }

    async fn on_receive_nick(&mut self, nickname: &str) -> Result<()> {
        if nickname.len() > settings::MAXIMUM_LENGTH_OF_NICKNAME {
            return Ok(())
        }

        self.irc_config.is_nick_init = true;
        let old_nick = std::mem::replace(&mut self.irc_config.nickname, nickname.to_string());

        let nick_reply =
            format!(":{}!anon@dark.fi NICK {}\r\n", old_nick, self.irc_config.nickname);
        self.reply(&nick_reply).await
    }

    async fn on_receive_part(&mut self, channels: Vec<String>) -> Result<()> {
        for chan in channels.iter() {
            let part_reply =
                format!(":{}!anon@dark.fi PART {}\r\n", self.irc_config.nickname, chan);
            self.reply(&part_reply).await?;
            if self.irc_config.channels.contains_key(chan) {
                let chan_info = self.irc_config.channels.get_mut(chan).unwrap();
                chan_info.joined = false;
            }
        }
        Ok(())
    }

    async fn on_receive_topic(&mut self, line: &str, channel: &str) -> Result<()> {
        if let Some(substr_idx) = line.find(':') {
            // Client is setting the topic
            if substr_idx >= line.len() {
                return Err(Error::MalformedPacket)
            }

            let topic = &line[substr_idx + 1..];
            let chan_info = self.irc_config.channels.get_mut(channel).unwrap();
            chan_info.topic = Some(topic.to_string());

            let topic_reply = format!(
                ":{}!anon@dark.fi TOPIC {} :{}\r\n",
                self.irc_config.nickname, channel, topic
            );
            self.reply(&topic_reply).await?;
        } else {
            // Client is asking or the topic
            let chan_info = self.irc_config.channels.get(channel).unwrap();
            let topic_reply = if let Some(topic) = &chan_info.topic {
                format!(
                    "{} {} {} :{}\r\n",
                    RPL::Topic as u32,
                    self.irc_config.nickname,
                    channel,
                    topic
                )
            } else {
                const TOPIC: &str = "No topic is set";
                format!(
                    "{} {} {} :{}\r\n",
                    RPL::NoTopic as u32,
                    self.irc_config.nickname,
                    channel,
                    TOPIC
                )
            };
            self.reply(&topic_reply).await?;
        }
        Ok(())
    }

    async fn on_ping(&mut self, value: &str) -> Result<()> {
        let pong = format!("PONG {}\r\n", value);
        self.reply(&pong).await
    }

    async fn on_receive_cap(&mut self, line: &str, subcommand: &str) -> Result<()> {
        self.irc_config.is_cap_end = false;

        let capabilities_keys: Vec<String> = self.irc_config.capabilities.keys().cloned().collect();

        match subcommand {
            "LS" => {
                let cap_ls_reply = format!(
                    ":{}!anon@dark.fi CAP * LS :{}\r\n",
                    self.irc_config.nickname,
                    capabilities_keys.join(" ")
                );
                self.reply(&cap_ls_reply).await?;
            }

            "REQ" => {
                let substr_idx = line.find(':').ok_or(Error::MalformedPacket)?;

                if substr_idx >= line.len() {
                    return Err(Error::MalformedPacket)
                }

                let cap: Vec<&str> = line[substr_idx + 1..].split(' ').collect();

                let mut ack_list = vec![];
                let mut nak_list = vec![];

                for c in cap {
                    if self.irc_config.capabilities.contains_key(c) {
                        self.irc_config.capabilities.insert(c.to_string(), true);
                        ack_list.push(c);
                    } else {
                        nak_list.push(c);
                    }
                }

                let cap_ack_reply = format!(
                    ":{}!anon@dark.fi CAP * ACK :{}\r\n",
                    self.irc_config.nickname,
                    ack_list.join(" ")
                );

                let cap_nak_reply = format!(
                    ":{}!anon@dark.fi CAP * NAK :{}\r\n",
                    self.irc_config.nickname,
                    nak_list.join(" ")
                );

                self.reply(&cap_ack_reply).await?;
                self.reply(&cap_nak_reply).await?;
            }

            "LIST" => {
                let enabled_capabilities: Vec<String> = self
                    .irc_config
                    .capabilities
                    .clone()
                    .into_iter()
                    .filter(|(_, v)| *v)
                    .map(|(k, _)| k)
                    .collect();

                let cap_list_reply = format!(
                    ":{}!anon@dark.fi CAP * LIST :{}\r\n",
                    self.irc_config.nickname,
                    enabled_capabilities.join(" ")
                );
                self.reply(&cap_list_reply).await?;
            }

            "END" => {
                self.irc_config.is_cap_end = true;
            }
            _ => {}
        }
        Ok(())
    }

    async fn on_receive_names(&mut self, channels: Vec<String>) -> Result<()> {
        for chan in channels.iter() {
            if !chan.starts_with('#') {
                continue
            }
            if self.irc_config.channels.contains_key(chan) {
                let chan_info = self.irc_config.channels.get(chan).unwrap();

                if chan_info.names.is_empty() {
                    return Ok(())
                }

                let names_reply = format!(
                    ":{}!anon@dark.fi {} = {} : {}\r\n",
                    self.irc_config.nickname,
                    RPL::NameReply as u32,
                    chan,
                    chan_info.names.join(" ")
                );

                self.reply(&names_reply).await?;

                let end_of_names = format!(
                    ":DarkFi {:03} {} {} :End of NAMES list\r\n",
                    RPL::EndOfNames as u32,
                    self.irc_config.nickname,
                    chan
                );

                self.reply(&end_of_names).await?;
            }
        }
        Ok(())
    }

    async fn on_receive_privmsg(&mut self, line: &str, target: &str) -> Result<()> {
        let substr_idx = line.find(':').ok_or(Error::MalformedPacket)?;

        if substr_idx >= line.len() {
            return Err(Error::MalformedPacket)
        }

        let message = line[substr_idx + 1..].to_string();

        info!("[CLIENT {}] (Plain) PRIVMSG {} :{}", self.address, target, message,);

        let mut privmsg = PrivMsgEvent {
            nick: self.irc_config.nickname.clone(),
            target: target.to_string(),
            msg: message,
        };

        if target.starts_with('#') {
            if !self.irc_config.channels.contains_key(target) {
                return Ok(())
            }

            let channel_info = self.irc_config.channels.get(target).unwrap();

            if !channel_info.joined {
                return Ok(())
            }

            if let Some(salt_box) = &channel_info.salt_box(target) {
                encrypt_privmsg(salt_box, &mut privmsg);
                info!("[CLIENT {}] (Encrypted) PRIVMSG: {:?}", self.address, privmsg.to_string());
            }
        } else if self.irc_config.private_key.is_some() {
            if let Some(contact_info) = self.irc_config.contacts.get(target) {
                if let Some(salt_box) =
                    &contact_info.salt_box(self.irc_config.private_key.as_ref().unwrap(), target)
                {
                    encrypt_privmsg(salt_box, &mut privmsg);
                    info!(
                        "[CLIENT {}] (Encrypted) PRIVMSG: {:?}",
                        self.address,
                        privmsg.to_string()
                    );
                }
            }
        }

        self.server_notifier
            .send((NotifierMsg::Privmsg(privmsg), self.subscription.get_id()))
            .await?;

        Ok(())
    }

    async fn on_receive_join(&mut self, channels: Vec<String>) -> Result<()> {
        for chan in channels.iter() {
            if !chan.starts_with('#') {
                continue
            }
            if !self.irc_config.channels.contains_key(chan) {
                let mut chan_info = ChannelInfo::new();
                chan_info.topic = Some("n/a".to_string());
                self.irc_config.channels.insert(chan.to_string(), chan_info);
            }

            let chan_info = self.irc_config.channels.get_mut(chan).unwrap();
            if chan_info.joined {
                return Ok(())
            }
            chan_info.joined = true;

            let topic =
                if let Some(topic) = chan_info.topic.clone() { topic } else { "n/a".to_string() };
            chan_info.topic = Some(topic.to_string());

            {
                let j = format!(":{}!anon@dark.fi JOIN {}\r\n", self.irc_config.nickname, chan);
                let t = format!(":DarkFi TOPIC {} :{}\r\n", chan, topic);
                self.reply(&j).await?;
                self.reply(&t).await?;
            }
        }
        // Process missed messages if any (sorted by event's timestamp)
        let mut hash_vec = self.missed_events.lock().await.clone();
        hash_vec.sort_by(|a, b| a.timestamp.0.cmp(&b.timestamp.0));

        for event in hash_vec {
            let mut action = event.action.clone();
            if let Err(e) = self.process_msg(&mut action).await {
                error!("[CLIENT {}] Process msg: {}", self.address, e);
                continue
            }
        }
        Ok(())
    }
}

//
// Helper functions
//
fn clean_input_line(mut line: String) -> Result<String> {
    if line.is_empty() {
        return Err(Error::ChannelStopped)
    }

    if line == "\n" || line == "\r\n" {
        return Err(Error::ChannelStopped)
    }

    if &line[(line.len() - 2)..] == "\r\n" {
        // Remove CRLF
        line.pop();
        line.pop();
    } else if &line[(line.len() - 1)..] == "\n" {
        line.pop();
    } else {
        return Err(Error::ChannelStopped)
    }

    Ok(line.clone())
}

fn parse_line(line: &str) -> Result<(String, String)> {
    let mut tokens = line.split_ascii_whitespace();
    // Commands can begin with :garbage but we will reject clients doing
    // that for now to keep the protocol simple and focused.
    let command = tokens.next().ok_or(Error::MalformedPacket)?.to_uppercase();
    let value = tokens.next().ok_or(Error::MalformedPacket)?;
    Ok((command, value.to_owned()))
}
