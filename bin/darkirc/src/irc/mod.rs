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

use std::{collections::HashSet, sync::Arc};

use crypto_box::ChaChaBox;
use darkfi::{Error, Result};
use darkfi_serial::{async_trait, deserialize_async_partial, SerialDecodable, SerialEncodable};

/// IRC client state
pub(crate) mod client;

/// IRC server implementation
pub(crate) mod server;

/// IRC command handler
pub(crate) mod command;

/// Services implementations
pub(crate) mod services;
pub(crate) use services::nickserv::NickServ;

/// IRC numerics and server replies
pub(crate) mod rpl;

/// Hardcoded server name
const SERVER_NAME: &str = "irc.dark.fi";

pub trait Priv {
    fn channel(&mut self) -> &mut String;
    fn nick(&mut self) -> &mut String;
    fn msg(&mut self) -> &mut String;
}

/// IRC PRIVMSG (old version)
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct OldPrivmsg {
    pub channel: String,
    pub nick: String,
    pub msg: String,
}

impl OldPrivmsg {
    pub fn into_new(self) -> Privmsg {
        Privmsg {
            version: 0,
            msg_type: 0,
            channel: self.channel.clone(),
            nick: self.nick.clone(),
            msg: self.msg.clone(),
        }
    }
}

/// IRC PRIVMSG (new version)
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct Privmsg {
    pub version: u8,
    pub msg_type: u8,
    pub channel: String,
    pub nick: String,
    pub msg: String,
}

impl Priv for OldPrivmsg {
    fn channel(&mut self) -> &mut String {
        &mut self.channel
    }

    fn nick(&mut self) -> &mut String {
        &mut self.nick
    }

    fn msg(&mut self) -> &mut String {
        &mut self.msg
    }
}
impl Priv for Privmsg {
    fn channel(&mut self) -> &mut String {
        &mut self.channel
    }

    fn nick(&mut self) -> &mut String {
        &mut self.nick
    }

    fn msg(&mut self) -> &mut String {
        &mut self.msg
    }
}

pub enum Msg {
    V1(OldPrivmsg),
    V2(Privmsg),
}

impl Msg {
    pub async fn deserialize(bytes: &[u8]) -> Result<Self> {
        let old_privmsg = deserialize_async_partial(bytes).await;
        if let Ok((old_msg, _)) = old_privmsg {
            return Ok(Msg::V1(old_msg))
        }

        let new_privmsg = deserialize_async_partial(bytes).await;
        if let Ok((new_msg, _)) = new_privmsg {
            return Ok(Msg::V2(new_msg))
        }

        Err(Error::Custom("Unknown message format".into()))
    }
}

/// IRC channel definition
#[derive(Clone)]
pub struct IrcChannel {
    pub topic: String,
    pub nicks: HashSet<String>,
    pub saltbox: Option<Arc<ChaChaBox>>,
}

/// IRC contact definition
#[derive(Clone)]
pub struct IrcContact {
    pub saltbox: Option<Arc<ChaChaBox>>,
}
