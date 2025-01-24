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
use darkfi_sdk::crypto::{schnorr::SchnorrSecret, PublicKey, SecretKey};
use darkfi_serial::{
    async_trait, deserialize_async, serialize, Encodable, SerialDecodable, SerialEncodable,
};

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

/// IRC moderation message.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct Modmsg {
    /// Channel this message is for
    pub channel: String,
    /// The moderation command
    pub command: String,
    /// Command parameters
    pub params: String,
    /// Signature bytes of the moderation message hash,
    /// for validating moderator.
    pub signature: Vec<u8>,
}

impl Modmsg {
    // Generate a new `Modmsg` and sign it using provided secret key.
    pub fn new(channel: String, command: String, params: String, secret_key: &SecretKey) -> Self {
        let mut message = Self { channel, command, params, signature: vec![] };
        message.signature = serialize(&secret_key.sign(&message.hash()));
        message
    }

    /// Compute the moderation message hash. This hash consist of the blake3::Hash
    /// of the message `channel`, `command` and `params`.
    pub fn hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();

        // Blake3 hasher .update() method never fails.
        // This call returns a Result due to how the Write trait is specified.
        // Calling unwrap() here should be safe.
        self.channel.encode(&mut hasher).expect("blake3 hasher");
        self.command.encode(&mut hasher).expect("blake3 hasher");
        self.params.encode(&mut hasher).expect("blake3 hasher");

        hasher.finalize().into()
    }
}

pub enum Msg {
    V1(OldPrivmsg),
    V2(Privmsg),
    Mod(Modmsg),
}

impl Msg {
    pub async fn deserialize(bytes: &[u8]) -> Result<Self> {
        if let Ok(old_msg) = deserialize_async(bytes).await {
            return Ok(Msg::V1(old_msg))
        }

        if let Ok(new_msg) = deserialize_async(bytes).await {
            return Ok(Msg::V2(new_msg))
        }

        if let Ok(mod_msg) = deserialize_async(bytes).await {
            return Ok(Msg::Mod(mod_msg))
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
    pub moderators: Vec<PublicKey>,
    pub mod_secret_key: Option<SecretKey>,
}

/// IRC contact definition
#[derive(Clone)]
pub struct IrcContact {
    pub saltbox: Option<Arc<ChaChaBox>>,
}
