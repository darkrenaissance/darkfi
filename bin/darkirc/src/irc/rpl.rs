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

#![allow(clippy::zero_prefixed_literal)]

/// The welcome message sent upon successful registration
pub const WELCOME: &str = "Welcome to the DarkIRC network";
/// The message sent to the client when they are not registered
pub const NOT_REGISTERED: &str = "You have not registered";
/// The message sent to the client when they are already registered
pub const ALREADY_REGISTERED: &str = "You may not reregister";
/// The message sent to the client when they enter wrong or no password
pub const PASSWORD_MISMATCH: &str = "Password incorrect";
/// The message sent to the client when command params could not parse
pub const INVALID_SYNTAX: &str = "Syntax error";

/// `<client> :Welcome to the DarkIRC network`
///
/// The first message sent after client registration.
pub const RPL_WELCOME: u16 = 001;

/// `<client> :Your host is <servername>, running version <version>`
///
/// Part of the post-registration greeting.
pub const RPL_YOURHOST: u16 = 002;

/// `<client> <user modes>`
///
/// Sent to a client to inform that client of their currently-set user modes.
pub const RPL_UMODEIS: u16 = 221;

/// `<client> [<server>] :Administrative info`
///
/// Sent as a reply to an ADMIN command, this numeric establishes the
/// name of the server whose administrative info is being provided.
pub const RPL_ADMINME: u16 = 256;

/// `<client> :<info>`
///
/// Sent as a reply to an ADMIN command. `<info>` is a string intended to
/// provide information about the location of the server.
pub const RPL_ADMINLOC1: u16 = 257;

/// `<client> :<info>`
///
/// Sent as a reply to an ADMIN command. `<info>` is a string intended to
/// provide information about whoever runs the server.
pub const RPL_ADMINLOC2: u16 = 258;

/// `<client> :<info>`
///
/// Sent as a reply to an ADMIN command. `<info>` MUST contain the email
/// address to contact the administrator(s) of the server.
pub const RPL_ADMINEMAIL: u16 = 259;

/// `<client> Channel :Users  Name`
///
/// Sent as a reply to the LIST command, this numeric marks the start
/// of a channel list.
pub const RPL_LISTSTART: u16 = 321;

/// `<client> <channel> <client count> :<topic>`
///
/// Sent as a reply to the LIST command, this numeric sends information
/// about a channel to the client. `<channel>` is the name of the channel.
/// `<client count>` is an integer indicating how many clients are joined
/// to that channel. `<topic>` is the channel’s topic.
pub const RPL_LIST: u16 = 322;

/// `<client> :End of /LIST`
///
/// Sent as a reply to the LIST command, this numeric indicates the end
/// of a LIST response.
pub const RPL_LISTEND: u16 = 323;

/// `<client> <channel> <modestring> <mode arguments>...`
///
/// Sent to a client to inform them of the currently-set modes of a channel.
/// `<channel>` is the name of the channel.
pub const RPL_CHANNELMODEIS: u16 = 324;

/// `<client> <channel> :No topic is set`
///
/// Sent to a client when joining a channel to inform them that the channel
/// with the name `<channel>` does not have any topic set.
pub const RPL_NOTOPIC: u16 = 331;

/// `<client> <channel> :<topic>`
///
/// Sent to a client when joining the `<channel>` to inform them of the
/// current topic of the channel.
pub const RPL_TOPIC: u16 = 332;

/// `<client> <version> <server> :<comments>`
///
/// Sent as a reply to the VERSION command.
pub const RPL_VERSION: u16 = 351;

/// `<client> <symbol> <channel> :[prefix]<nick>{ [prefix]<nick>}`
///
/// Sent as a reply to the NAMES command
pub const RPL_NAMREPLY: u16 = 353;

/// `<client> <channel> :End of /NAMES list`
///
/// Sent as a reply to the NAMES command
pub const RPL_ENDOFNAMES: u16 = 366;

/// `<client> :<string>`
///
/// Sent as the reply to the INFO command.
pub const RPL_INFO: u16 = 371;

/// `<client> :End of INFO list`
///
/// Indicates the end of an INFO response.
pub const RPL_ENDOFINFO: u16 = 374;

/// `<client> :- <server> Message of the day -`
///
/// Indicates the start of the Message of the Day to the client.
pub const RPL_MOTDSTART: u16 = 375;

/// `<client> :<line of the motd>`
///
/// When sending the Message of the Day to the client, servers reply
/// with each line of the MOTD as this numeric.
pub const RPL_MOTD: u16 = 372;

/// `<client> :End of /MOTD command.`
///
/// Indicates the end of the Message of the Day to the client.
pub const RPL_ENDOFMOTD: u16 = 376;

/// `<client> <config file> :Rehashing`
///
/// Sent to an operator which has just successfully issued a REHASH
/// command.
pub const RPL_REHASHING: u16 = 382;

/// `<client> <nickname> :No such nick/channel`
///
/// Indicates that no client can be found for the supplied nickname.
pub const ERR_NOSUCHNICK: u16 = 401;

/// `<client> <channel> :No such channel`
///
/// Indicates that no channel can be found for the supplied channel name.
pub const ERR_NOSUCHCHANNEL: u16 = 403;

/// `<client> :No origin specified`
///
/// Indicates a PING or PONG message missing the originator parameter
/// which is required by old IRC servers. Nowadays, this may be used by
/// some servers when the PING `<token>` is empty.
pub const ERR_NOORIGIN: u16 = 409;

/// `<client> :No recipient given (<command>)`
///
/// Returned by the PRIVMSG command to indicate the message wasn’t
/// delivered because there was no recipient given.
pub const ERR_NORECIPIENT: u16 = 411;

/// `<client> :No text to send`
///
/// Returned by the PRIVMSG command to indicate the message wasn’t
/// delivered because there was no text to send.
pub const ERR_NOTEXTTOSEND: u16 = 412;

/// `<client> <nick> :Erroneus nickname`
///
/// Returned when a NICK command cannot be successfully completed as
/// the desired nickname contains characters that are disallowed by the server.
pub const ERR_ERRONEOUSNICKNAME: u16 = 432;

/// `<client> :You have not registered`
///
/// Returned when a client command cannot be parsed because they are
/// not registered.
pub const ERR_NOTREGISTERED: u16 = 451;

/// `<client> <command> :Not enough parameters`
///
/// Returned when a client command cannot be parsed because not enough
/// parameters were supplied.
pub const ERR_NEEDMOREPARAMS: u16 = 461;

/// `<client> :You may not reregister`
///
/// Returned when a client tries to change a detail that can only be
/// set during registration.
pub const ERR_ALREADYREGISTERED: u16 = 462;

/// `<client> :Password incorrect`
///
/// Returned to indicate that the connection could not be registered
/// as the password was either incorrect or not supplied.
pub const ERR_PASSWDMISMATCH: u16 = 464;

/// `<client> :Cant change mode for other users`
///
/// Indicates that a MODE command affecting a user failed because they
/// were trying to set or view modes for other users.
pub const ERR_USERSDONTMATCH: u16 = 502;
