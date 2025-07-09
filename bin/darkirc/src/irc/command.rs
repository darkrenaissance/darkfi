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

//! IRC command implemenatations
//!
//! These try to follow the RFCs, modified in order for our P2P stack.
//! Copied from <https://simple.wikipedia.org/wiki/List_of_Internet_Relay_Chat_commands>
//!
//! Unimplemented commands:
//! * `AWAY`
//! * `CONNECT`
//! * `DIE`
//! * `ERROR`
//! * `INVITE`
//! * `ISON`
//! * `KICK`
//! * `KILL`
//! * `NOTICE`
//! * `OPER`
//! * `RESTART`
//! * `SERVICE`
//! * `SERVLIST`
//! * `SERVER`
//! * `SQUERY`
//! * `SQUIT`
//! * `SUMMON`
//! * `TRACE`
//! * `USERHOST`
//! * `WALLOPS`
//! * `WHO`
//! * `WHOIS`
//! * `WHOWAS`
//!
//! Some of the above commands could actually be implemented and could
//! work in respect to the P2P network.

use std::{collections::HashSet, sync::atomic::Ordering::SeqCst};

use darkfi::Result;
use tracing::{error, info};

use super::{
    client::{Client, ReplyType},
    rpl::*,
    server::MAX_NICK_LEN,
    IrcChannel, Msg, SERVER_NAME,
};
use crate::crypto::bcrypt::bcrypt_hash_password;

impl Client {
    /// `ADMIN [<server>]`
    ///
    /// Asks the server for information about the administrator of the server.
    pub async fn handle_cmd_admin(&self, _args: &str) -> Result<Vec<ReplyType>> {
        if !self.registered.load(SeqCst) {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((ERR_NOTREGISTERED, format!("* :{NOT_REGISTERED}")))])
        }

        let nick = self.nickname.read().await.to_string();

        let replies = vec![
            ReplyType::Server((RPL_ADMINME, format!("{nick} {SERVER_NAME} :Administrative info"))),
            ReplyType::Server((RPL_ADMINLOC1, format!("{nick} :"))),
            ReplyType::Server((RPL_ADMINLOC2, format!("{nick} :"))),
            ReplyType::Server((RPL_ADMINEMAIL, format!("{nick} :anon@darkirc"))),
        ];

        Ok(replies)
    }

    /// `CAP <args>`
    pub async fn handle_cmd_cap(&self, args: &str) -> Result<Vec<ReplyType>> {
        let mut tokens = args.split_ascii_whitespace();

        let Some(subcommand) = tokens.next() else {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((
                ERR_NEEDMOREPARAMS,
                format!("{} CAP :{INVALID_SYNTAX}", self.nickname.read().await),
            ))])
        };

        let caps_keys: Vec<String> = self.caps.read().await.keys().cloned().collect();
        let nick = self.nickname.read().await.to_string();

        match subcommand.to_uppercase().as_str() {
            "LS" => {
                /*
                let Some(_version) = tokens.next() else {
                    return Ok(vec![ReplyType::Server((
                        ERR_NEEDMOREPARAMS,
                        format!("{} CAP :{INVALID_SYNTAX}", self.nickname.read().await),
                    ))])
                };
                */

                self.reg_paused.store(true, SeqCst);
                return Ok(vec![ReplyType::Cap(format!("CAP * LS :{}", caps_keys.join(" ")))])
            }

            "REQ" => {
                let Some(substr_idx) = args.find(':') else {
                    return Ok(vec![ReplyType::Server((
                        ERR_NEEDMOREPARAMS,
                        format!("{nick} CAP :{INVALID_SYNTAX}"),
                    ))])
                };

                if substr_idx >= args.len() {
                    return Ok(vec![ReplyType::Server((
                        ERR_NEEDMOREPARAMS,
                        format!("{nick} CAP :{INVALID_SYNTAX}"),
                    ))])
                }

                let cap_reqs: Vec<&str> = args[substr_idx + 1..].split(' ').collect();

                let mut ack_list = vec![];
                let mut nak_list = vec![];

                let mut available_caps = self.caps.write().await;
                for cap in cap_reqs {
                    if available_caps.contains_key(cap) {
                        available_caps.insert(cap.to_string(), true);
                        ack_list.push(cap);
                    } else {
                        nak_list.push(cap);
                    }
                }

                let mut replies = vec![];

                if !ack_list.is_empty() {
                    replies.push(ReplyType::Cap(format!("CAP {nick} ACK :{}", ack_list.join(" "))));
                }

                if !nak_list.is_empty() {
                    replies.push(ReplyType::Cap(format!("CAP {nick} NAK :{}", nak_list.join(" "))));
                }

                return Ok(replies)
            }

            "LIST" => {
                let enabled_caps: Vec<String> = self
                    .caps
                    .read()
                    .await
                    .clone()
                    .into_iter()
                    .filter(|(_, v)| *v)
                    .map(|(k, _)| k)
                    .collect();

                return Ok(vec![ReplyType::Cap(format!(
                    "CAP {nick} LIST :{}",
                    enabled_caps.join(" ")
                ))])
            }

            "END" => {
                // At CAP END, if we have USER and NICK, we can welcome them.
                self.reg_paused.store(false, SeqCst);
                if self.registered.load(SeqCst) && !self.is_cap_end.load(SeqCst) {
                    self.is_cap_end.store(true, SeqCst);
                    return Ok(self.welcome().await)
                }

                return Ok(vec![])
            }

            _ => {}
        }

        self.penalty.fetch_add(1, SeqCst);
        Ok(vec![ReplyType::Server((ERR_NEEDMOREPARAMS, format!("{nick} CAP :{INVALID_SYNTAX}")))])
    }

    /// `INFO [<target>]`
    ///
    /// Gives information about the `<target>` server, or the current server if
    /// `<target>` is not used. The information includes the server's version,
    /// when it was compiled, the patch level, when it was started, and any
    /// other information which might be relevant.
    pub async fn handle_cmd_info(&self, _args: &str) -> Result<Vec<ReplyType>> {
        if !self.registered.load(SeqCst) {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((ERR_NOTREGISTERED, format!("* :{NOT_REGISTERED}")))])
        }

        let nick = self.nickname.read().await.clone();
        let replies = vec![
            ReplyType::Server((RPL_INFO, format!("{nick} :DarkIRC {}", env!("CARGO_PKG_VERSION")))),
            ReplyType::Server((RPL_ENDOFINFO, format!("{nick} :End of INFO list"))),
        ];

        Ok(replies)
    }

    /// `JOIN <channels> [<keys>]`
    ///
    /// Makes the client join the channels in the list `<channels>`.
    /// Passwords can be used in the list `<keys>`. If the channels do not
    /// exist, they will be created.
    pub async fn handle_cmd_join(&self, args: &str, hist: bool) -> Result<Vec<ReplyType>> {
        if !self.registered.load(SeqCst) {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((ERR_NOTREGISTERED, format!("* :{NOT_REGISTERED}")))])
        }

        // Client's (already) active channels
        let mut active_channels = self.channels.write().await;
        // Here we'll hold valid channel names.
        let mut channels = HashSet::new();

        // Let's scan through our channels. For now we'll only support
        // channel names starting with a single '#' character.
        let nick = self.nickname.read().await.to_string();
        let tokens = args.split_ascii_whitespace();
        for channel in tokens {
            if !channel.starts_with('#') {
                self.penalty.fetch_add(1, SeqCst);
                return Ok(vec![ReplyType::Server((
                    ERR_NEEDMOREPARAMS,
                    format!("{nick} JOIN :{INVALID_SYNTAX}"),
                ))])
            }

            if !active_channels.contains(channel) {
                channels.insert(channel.to_string());
            }
        }

        // Weechat sends channels as `#chan1,#chan2,#chan3`. Handle it.
        if channels.len() == 1 {
            let list = channels.iter().next().unwrap().clone();
            channels.remove(list.as_str());

            for channel in list.split(',') {
                if !channel.starts_with('#') || channel.len() > MAX_NICK_LEN {
                    self.penalty.fetch_add(1, SeqCst);
                    return Ok(vec![ReplyType::Server((
                        ERR_NEEDMOREPARAMS,
                        format!("{nick} JOIN :{INVALID_SYNTAX}"),
                    ))])
                }
                if !active_channels.contains(channel) {
                    channels.insert(channel.to_string());
                }
            }
        }

        // Create new channels for this client and construct replies.
        let mut server_channels = self.server.channels.write().await;
        let mut replies = vec![];

        for channel in channels.iter() {
            // Insert the channel name into the set of client's active channels
            active_channels.insert(channel.clone());
            // Create or update the channel on the server side.
            if let Some(server_chan) = server_channels.get_mut(channel) {
                server_chan.nicks.insert(nick.clone());
            } else {
                let chan = IrcChannel {
                    topic: String::new(),
                    nicks: HashSet::from([nick.clone()]),
                    saltbox: None,
                };
                server_channels.insert(channel.clone(), chan);
            }

            // Create the replies
            replies.push(ReplyType::Client((nick.clone(), format!("JOIN :{channel}"))));

            if let Some(chan) = server_channels.get(channel) {
                if !chan.topic.is_empty() {
                    replies.push(ReplyType::Client((
                        nick.clone(),
                        format!("TOPIC {channel} :{}", chan.topic),
                    )));
                }
            }
        }

        // Drop the locks as they're used in get_history()
        drop(active_channels);
        drop(server_channels);

        if hist {
            // Potentially extend the replies with channel history
            replies.extend(self.get_history(&channels).await.unwrap());
        }

        Ok(replies)
    }

    /// `LIST [<channels> [<server>]]`
    ///
    /// List all channels on the server. If the list `<channels>` is given, it
    /// will return the channel topics. If `<server>` is given, the command will
    /// be sent to `<server>` for evaluation.
    pub async fn handle_cmd_list(&self, _args: &str) -> Result<Vec<ReplyType>> {
        if !self.registered.load(SeqCst) {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((ERR_NOTREGISTERED, format!("* :{NOT_REGISTERED}")))])
        }

        let nick = self.nickname.read().await.to_string();

        let mut list = vec![];
        for (name, channel) in self.server.channels.read().await.iter() {
            list.push(format!("{nick} {name} {} :{}", channel.nicks.len(), channel.topic));
        }

        let mut replies = vec![];
        replies.push(ReplyType::Server((RPL_LISTSTART, format!("{nick} Channel :Users  Name"))));
        for chan in list {
            replies.push(ReplyType::Server((RPL_LIST, chan)));
        }
        replies.push(ReplyType::Server((RPL_LISTEND, format!("{nick} :End of /LIST"))));

        Ok(replies)
    }

    /// `MODE <nickname> <flags>`
    /// `MODE <channel> <flags>`
    ///
    /// The MODE command has two uses. It can be used to set both user and
    /// channel modes.
    pub async fn handle_cmd_mode(&self, args: &str) -> Result<Vec<ReplyType>> {
        if !self.registered.load(SeqCst) {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((ERR_NOTREGISTERED, format!("* :{NOT_REGISTERED}")))])
        }

        let nick = self.nickname.read().await.to_string();

        let mut tokens = args.split_ascii_whitespace();

        let Some(target) = tokens.next() else {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((
                ERR_NEEDMOREPARAMS,
                format!("{nick} MODE :{INVALID_SYNTAX}"),
            ))])
        };

        if target == nick {
            return Ok(vec![ReplyType::Server((RPL_UMODEIS, format!("{nick} +")))])
        }

        if !target.starts_with('#') {
            return Ok(vec![ReplyType::Server((
                ERR_USERSDONTMATCH,
                format!("{nick} :Can't set/get mode for other users"),
            ))])
        }

        if !self.server.channels.read().await.contains_key(target) {
            return Ok(vec![ReplyType::Server((
                ERR_NOSUCHNICK,
                format!("{nick} {target} :No such nick or channel name"),
            ))])
        }

        Ok(vec![ReplyType::Server((RPL_CHANNELMODEIS, format!("{nick} {target} +")))])
    }

    /// `MOTD [<server>]`
    ///
    /// Returns the message of the day on `<server>` or the current server if
    /// it is not stated.
    pub async fn handle_cmd_motd(&self, _args: &str) -> Result<Vec<ReplyType>> {
        let nick = self.nickname.read().await.to_string();

        Ok(vec![
            ReplyType::Server((
                RPL_MOTDSTART,
                format!("{nick} :- {SERVER_NAME} message of the day"),
            )),
            ReplyType::Server((RPL_MOTD, format!("{nick} :Let there be dark!"))),
            ReplyType::Server((RPL_ENDOFMOTD, format!("{nick} :End of /MOTD command."))),
        ])
    }

    /// `NAMES [<channel>]`
    ///
    /// Returns a list of who is on the list of `<channel>`, by channel name.
    /// If `<channel>` is not used, all users are shown. They are grouped by
    /// channel name with all users who are not on a channel being shown as
    /// part of channel "*".
    pub async fn handle_cmd_names(&self, args: &str) -> Result<Vec<ReplyType>> {
        if !self.registered.load(SeqCst) {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((ERR_NOTREGISTERED, format!("* :{NOT_REGISTERED}")))])
        }

        let nick = self.nickname.read().await.to_string();
        let mut tokens = args.split_ascii_whitespace();
        let mut replies = vec![];

        // If a channel was requested, reply only with that one.
        // Otherwise, return info for all known channels.
        if let Some(req_chan) = tokens.next() {
            if let Some(chan) = self.server.channels.read().await.get(req_chan) {
                let nicks: Vec<String> = chan.nicks.iter().cloned().collect();

                replies.push(ReplyType::Server((
                    RPL_NAMREPLY,
                    format!("{nick} = {req_chan} :{}", nicks.join(" ")),
                )));
            }

            replies.push(ReplyType::Server((
                RPL_ENDOFNAMES,
                format!("{nick} {req_chan} :End of NAMES list"),
            )));

            Ok(replies)
        } else {
            for (name, chan) in self.server.channels.read().await.iter() {
                let nicks: Vec<String> = chan.nicks.iter().cloned().collect();

                replies.push(ReplyType::Server((
                    RPL_NAMREPLY,
                    format!("{nick} = {name} :{}", nicks.join(" ")),
                )));
            }

            replies
                .push(ReplyType::Server((RPL_ENDOFNAMES, format!("{nick} * :End of NAMES list"))));

            Ok(replies)
        }
    }

    /// `NICK <nickname>`
    ///
    /// Allows a client to change their IRC nickname.
    pub async fn handle_cmd_nick(&self, args: &str) -> Result<Vec<ReplyType>> {
        // Parse the line
        let mut tokens = args.split_ascii_whitespace();

        // Reference the current nickname
        let old_nick = self.nickname.read().await.to_string();

        let Some(nickname) = tokens.next() else {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((
                ERR_NEEDMOREPARAMS,
                format!("{old_nick} NICK :{INVALID_SYNTAX}"),
            ))])
        };

        // Forbid disallowed characters.
        // The next() call is done to check for ASCII whitespace in the nick.
        if tokens.next().is_some() || nickname.starts_with(':') || nickname.starts_with('#') {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((
                ERR_ERRONEOUSNICKNAME,
                format!("{old_nick} {nickname} :Erroneous nickname"),
            ))])
        }

        // Disallow too long nicks
        if nickname.len() > MAX_NICK_LEN {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((
                ERR_ERRONEOUSNICKNAME,
                format!("{old_nick} {nickname} :Nickname too long"),
            ))])
        }

        // Set the new nickname
        *self.nickname.write().await = nickname.to_string();

        // If the username is set, we can complete the registration
        if *self.username.read().await != "*" &&
            !self.registered.load(SeqCst) &&
            self.is_pass_set.load(SeqCst)
        {
            self.registered.store(true, SeqCst);
            if self.reg_paused.load(SeqCst) {
                return Ok(vec![])
            } else {
                return Ok(self.welcome().await)
            }
        }

        // If we were registered, we send a client reply about it.
        if self.registered.load(SeqCst) {
            Ok(vec![ReplyType::Client((old_nick, format!("NICK :{nickname}")))])
        } else {
            // Otherwise, we don't reply.
            Ok(vec![])
        }
    }

    /// `PART <channel>`
    ///
    /// Causes a user to leave the channel `<channel>`.
    pub async fn handle_cmd_part(&self, args: &str) -> Result<Vec<ReplyType>> {
        if !self.registered.load(SeqCst) {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((ERR_NOTREGISTERED, format!("* :{NOT_REGISTERED}")))])
        }

        let nick = self.nickname.read().await.to_string();
        let mut tokens = args.split_ascii_whitespace();

        let Some(channel) = tokens.next() else {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((
                ERR_NEEDMOREPARAMS,
                format!("{nick} PART :{INVALID_SYNTAX}"),
            ))])
        };

        if !channel.starts_with('#') {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((
                ERR_NEEDMOREPARAMS,
                format!("{nick} PART :{INVALID_SYNTAX}"),
            ))])
        }

        let mut active_channels = self.channels.write().await;
        if !active_channels.contains(channel) {
            return Ok(vec![ReplyType::Server((
                ERR_NOSUCHCHANNEL,
                format!("{nick} {channel} :No such channel"),
            ))])
        }

        // Remove the channel from the client's channel list
        active_channels.remove(channel);

        let replies = vec![ReplyType::Client((nick, format!("PART {channel} :Bye")))];

        Ok(replies)
    }

    /// `PASS <password>`
    ///
    /// Used to set a connection `<password>`. If set, the password must
    /// be set before USER/NICK commands.
    pub async fn handle_cmd_pass(&self, args: &str) -> Result<Vec<ReplyType>> {
        let nick = self.nickname.read().await.to_string();

        // "the final parameter can be prepended with a (':', 0x3A) character"
        // <https://modern.ircdocs.horse/#parameters>
        let Some(i) = args.find(' ') else {
            // self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((
                ERR_NEEDMOREPARAMS,
                format!("{nick} PASS :{INVALID_SYNTAX}"),
            ))])
        };
        let mut password = &args[i + 1..];
        if password.starts_with(':') {
            password = &password[1..];
        }

        if self.server.password == bcrypt_hash_password(password) {
            self.is_pass_set.store(true, SeqCst);
        } else {
            error!("[IRC CLIENT] Password is not correct!");
            return Ok(vec![ReplyType::Server((
                ERR_PASSWDMISMATCH,
                format!("{nick} PASS :{PASSWORD_MISMATCH}"),
            ))])
        }

        Ok(vec![])
    }

    /// `PING <server1>`
    ///
    /// Tests a connection. A PING message results in a PONG reply.
    pub async fn handle_cmd_ping(&self, args: &str) -> Result<Vec<ReplyType>> {
        if !self.registered.load(SeqCst) {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((ERR_NOTREGISTERED, format!("* :{NOT_REGISTERED}")))])
        }

        let mut tokens = args.split_ascii_whitespace();

        let Some(origin) = tokens.next() else {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((
                ERR_NOORIGIN,
                format!("{} :No origin specified", self.nickname.read().await),
            ))])
        };

        Ok(vec![ReplyType::Pong(origin.to_string())])
    }

    /// `PRIVMSG <msgtarget> <message>`
    ///
    /// Sends `<message>` to `<msgtarget>`. The target is usually a user or
    /// a channel.
    pub async fn handle_cmd_privmsg(&self, args: &str) -> Result<Vec<ReplyType>> {
        if !self.registered.load(SeqCst) {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((ERR_NOTREGISTERED, format!("* :{NOT_REGISTERED}")))])
        }

        let nick = self.nickname.read().await.to_string();
        let mut tokens = args.split_ascii_whitespace();

        let Some(target) = tokens.next() else {
            return Ok(vec![ReplyType::Server((
                ERR_NORECIPIENT,
                format!("{nick} :No recipient given (PRIVMSG)"),
            ))])
        };

        let Some(message) = tokens.next() else {
            return Ok(vec![ReplyType::Server((
                ERR_NOTEXTTOSEND,
                format!("{nick} :No text to send"),
            ))])
        };

        if !message.starts_with(':') || (message.trim() == ":" && tokens.next().is_none()) {
            return Ok(vec![ReplyType::Server((
                ERR_NOTEXTTOSEND,
                format!("{nick} :No text to send"),
            ))])
        }

        // We only send a client reply if the message is for ourself or if
        // we're trying to communicate with IRC services.
        // Anything else is rendered by the IRC client and not supposed
        // to be echoed by the IRC serer.
        if target == nick {
            return Ok(vec![ReplyType::Client((
                target.to_string(),
                format!("PRIVMSG {target} {message}"),
            ))])
        }

        // Handle queries to NickServ
        if target.to_lowercase().as_str() == "nickserv" {
            return self.nickserv.handle_query(message.strip_prefix(':').unwrap()).await
        }

        // If it's a DM and we don't have an encryption key, we will
        // refuse to send it. Send ERR_NORECIPIENT to the client.
        if !target.starts_with('#') && !self.server.contacts.read().await.contains_key(target) {
            return Ok(vec![ReplyType::Server((ERR_NOSUCHNICK, format!("{nick} :{target}")))])
        }

        Ok(vec![])
    }

    /// `REHASH`
    ///
    /// Causes the server to re-read and re-process its configuration file(s).
    pub async fn handle_cmd_rehash(&self, _args: &str) -> Result<Vec<ReplyType>> {
        info!("Attempting to rehash server...");
        if let Err(e) = self.server.rehash().await {
            error!("Failed to rehash server: {e}");
        }

        Ok(vec![ReplyType::Server((RPL_REHASHING, "Config reloaded!".to_string()))])
    }

    /// `TOPIC <channel> [<topic>]`
    ///
    /// Used to get the channel topic on `<channel>`. If `<topic>` is given, it
    /// sets the channel topic to `<topic>`.
    pub async fn handle_cmd_topic(&self, args: &str) -> Result<Vec<ReplyType>> {
        if !self.registered.load(SeqCst) {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((ERR_NOTREGISTERED, format!("* :{NOT_REGISTERED}")))])
        }

        let nick = self.nickname.read().await.to_string();
        let mut tokens = args.split_ascii_whitespace();

        let Some(channel) = tokens.next() else {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((
                ERR_NEEDMOREPARAMS,
                format!("{nick} TOPIC :{INVALID_SYNTAX}"),
            ))])
        };

        if !self.server.channels.read().await.contains_key(channel) {
            return Ok(vec![ReplyType::Server((
                ERR_NOSUCHCHANNEL,
                format!("{nick} {channel} :No such channel"),
            ))])
        }

        // If there's a topic, we'll set it, otherwise return the set topic.
        let Some(topic) = tokens.next() else {
            let topic = self.server.channels.read().await.get(channel).unwrap().topic.clone();
            if topic.is_empty() {
                return Ok(vec![ReplyType::Server((
                    RPL_NOTOPIC,
                    format!("{nick} {channel} :No topic is set"),
                ))])
            } else {
                return Ok(vec![ReplyType::Server((
                    RPL_TOPIC,
                    format!("{nick} {channel} :{topic}"),
                ))])
            }
        };

        // Set the new topic
        self.server.channels.write().await.get_mut(channel).unwrap().topic =
            topic.strip_prefix(':').unwrap().to_string();

        // Send reply
        let replies = vec![ReplyType::Client((nick, format!("TOPIC {channel} {topic}")))];

        Ok(replies)
    }

    /// `USER <user> <mode> <unused> <realname>`
    ///
    /// This command is used at the beginning of a connection to specify the
    /// username, hostname, real name, and the initial user modes of the
    /// connecting client. `<realname>` may contain spaces, and thus must be
    /// prefixed with a colon.
    pub async fn handle_cmd_user(&self, args: &str) -> Result<Vec<ReplyType>> {
        if self.registered.load(SeqCst) {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((
                ERR_ALREADYREGISTERED,
                format!("{} :{ALREADY_REGISTERED}", self.nickname.read().await),
            ))])
        }

        // If password is not set register user normally
        if self.server.password.is_empty() {
            self.is_pass_set.store(true, SeqCst);
        }

        // Parse the line
        let nick = self.nickname.read().await.to_string();
        let mut tokens = args.split_ascii_whitespace();

        let Some(username) = tokens.next() else {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((
                ERR_NEEDMOREPARAMS,
                format!("{nick} USER :{INVALID_SYNTAX}"),
            ))])
        };

        // Mode syntax is currently ignored, but should be part of the command
        let Some(_mode) = tokens.next() else {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((
                ERR_NEEDMOREPARAMS,
                format!("{nick} USER :{INVALID_SYNTAX}"),
            ))])
        };

        // Next token is unused per RFC, but should be part of the command
        let Some(_unused) = tokens.next() else {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((
                ERR_NEEDMOREPARAMS,
                format!("{nick} USER :{INVALID_SYNTAX}"),
            ))])
        };

        // The final token should be realname and should start with a colon
        let Some(realname) = tokens.next() else {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((
                ERR_NEEDMOREPARAMS,
                format!("{nick} USER :{INVALID_SYNTAX}"),
            ))])
        };

        if !realname.starts_with(':') {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((
                ERR_NEEDMOREPARAMS,
                format!("{nick} USER :{INVALID_SYNTAX}"),
            ))])
        }

        *self.username.write().await = username.to_string();
        *self.realname.write().await = realname.to_string();

        // If the nickname is set, we can complete the registration
        if nick != "*" {
            if !self.is_pass_set.load(SeqCst) {
                return Ok(vec![ReplyType::Server((
                    ERR_PASSWDMISMATCH,
                    format!("{nick} PASS :{PASSWORD_MISMATCH}"),
                ))])
            }
            self.registered.store(true, SeqCst);
            if self.reg_paused.load(SeqCst) {
                return Ok(vec![])
            } else {
                return Ok(self.welcome().await)
            }
        }

        // Otherwise, we don't have to reply.
        Ok(vec![])
    }

    /// `VERSION`
    ///
    /// Returns the version of the server.
    pub async fn handle_cmd_version(&self, _args: &str) -> Result<Vec<ReplyType>> {
        if !self.registered.load(SeqCst) {
            self.penalty.fetch_add(1, SeqCst);
            return Ok(vec![ReplyType::Server((ERR_NOTREGISTERED, format!("* :{NOT_REGISTERED}")))])
        }

        let replies = vec![ReplyType::Server((
            RPL_VERSION,
            format!(
                "{} {} {SERVER_NAME} :Let there be dark!",
                self.nickname.read().await,
                env!("CARGO_PKG_VERSION")
            ),
        ))];

        Ok(replies)
    }

    /// Internal function that constructs the welcome message.
    async fn welcome(&self) -> Vec<ReplyType> {
        let nick = self.nickname.read().await.to_string();

        let mut replies = vec![
            ReplyType::Server((RPL_WELCOME, format!("{nick} :{WELCOME}"))),
            ReplyType::Server((
                RPL_YOURHOST,
                format!(
                    "{nick} :Your host is irc.dark.fi, running version {}",
                    env!("CARGO_PKG_VERSION")
                ),
            )),
        ];

        // Append the MOTD
        replies.append(&mut self.handle_cmd_motd("").await.unwrap());

        let mut channels = HashSet::new();

        // If we have any configured autojoin channels, let's join the user
        // and set their topics, if any.
        if !*self.caps.read().await.get("no-autojoin").unwrap() {
            for channel in self.server.autojoin.read().await.iter() {
                replies.extend(self.handle_cmd_join(channel, false).await.unwrap());
                channels.insert(channel.to_string());
            }
        }

        // Potentially extend the replies with channel history
        replies.extend(self.get_history(&channels).await.unwrap());

        // And request NAMES list.
        if !*self.caps.read().await.get("no-autojoin").unwrap() {
            for channel in self.server.autojoin.read().await.iter() {
                if let Some(chan) = self.server.channels.read().await.get(channel) {
                    let nicks: Vec<String> = chan.nicks.iter().cloned().collect();

                    replies.push(ReplyType::Server((
                        RPL_NAMREPLY,
                        format!("{nick} = {channel} :{}", nicks.join(" ")),
                    )));
                }

                replies.push(ReplyType::Server((
                    RPL_ENDOFNAMES,
                    format!("{nick} {channel} :End of NAMES list"),
                )));
            }
        }

        replies
    }

    /// Internal function that scans the DAG and returns events for
    /// given channels. Will return empty if no_history CAP is requested.
    // N.b. the handling of "live messages" is implemented
    // <file:./client.rs::r = self.incoming.receive().fuse() => {>
    // for which the logic for delivery should be kept in sync
    async fn get_history(&self, channels: &HashSet<String>) -> Result<Vec<ReplyType>> {
        if channels.is_empty() || *self.caps.read().await.get("no-history").unwrap() {
            return Ok(vec![])
        }

        // Fetch and order all the events from the DAG
        let dag_events = self.server.darkirc.event_graph.order_events().await;

        // Here we'll hold the events in order we'll push to the client
        let mut replies = vec![];

        for event in dag_events.iter() {
            let event_id = event.id();
            // If it was seen, skip
            match self.is_seen(&event_id).await {
                Ok(true) => continue,
                Ok(false) => {}
                Err(e) => {
                    error!("[IRC CLIENT] (get_history) self.is_seen({event_id}) failed: {e}");
                    return Err(e)
                }
            }

            // Try to deserialize it. (Here we skip errors)
            let mut privmsg = match Msg::deserialize(event.content()).await {
                Ok(Msg::V1(old_msg)) => old_msg.into_new(),
                Ok(Msg::V2(new_msg)) => new_msg,
                Err(_) => continue,
            };

            // Potentially decrypt the privmsg
            self.server.try_decrypt(&mut privmsg, self.nickname.read().await.as_ref()).await;

            // We should skip any attempts to contact services from the network.
            if ["nickserv", "chanserv"].contains(&privmsg.nick.to_lowercase().as_str()) {
                continue
            }

            // If the PRIVMSG is intended for any of the given
            // channels or contacts, add it as a reply and
            // mark it as seen in the seen_events tree.
            let contacts = self.server.contacts.read().await;
            if !channels.contains(&privmsg.channel) && !contacts.contains_key(&privmsg.channel) {
                continue
            }

            // Insert nicks into channels
            if let Some(chan) = self.server.channels.write().await.get_mut(&privmsg.channel) {
                chan.nicks.insert(privmsg.nick.clone());
            }

            // Handle message lines individually
            for line in privmsg.msg.lines() {
                // Skip empty lines
                if line.is_empty() {
                    continue;
                }

                // Format the message
                let msg = format!("PRIVMSG {} :{line}", privmsg.channel);

                // Send it to the client
                replies.push(ReplyType::Client((privmsg.nick.clone(), msg)));
            }

            // Mark the message as seen for this USER
            if let Err(e) = self.mark_seen(&event_id).await {
                error!("[IRC CLIENT] (get_history) self.mark_seen({event_id}) failed: {e}");
                return Err(e)
            }
        }

        Ok(replies)
    }
}
