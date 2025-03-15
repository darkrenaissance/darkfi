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

/// Hardcoded current Privmsg version
const PRIVMSG_VERSION: u8 = 0;

/// Privmsg types
const PRIVMSG_TYPE_NORMAL: u8 = 0;

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
            signature: vec![],
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
    /// Optional signature bytes of the message hash,
    /// for validating identity.
    pub signature: Vec<u8>,
}

impl Privmsg {
    // Generate a new `Privmsg` and sign it if a secret key is provided.
    pub fn new(
        msg_type: u8,
        channel: String,
        nick: String,
        msg: String,
        secret_key: Option<SecretKey>,
    ) -> Self {
        let mut message =
            Self { version: PRIVMSG_VERSION, msg_type, channel, nick, msg, signature: vec![] };
        if let Some(secret_key) = secret_key {
            message.signature = serialize(&secret_key.sign(&message.hash()));
        }
        message
    }

    /// Compute the message hash. This hash consist of the blake3::Hash
    /// of the message `version, `msg_type`, `channel`, `nick` and `msg`.
    pub fn hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();

        // Blake3 hasher .update() method never fails.
        // This call returns a Result due to how the Write trait is specified.
        // Calling unwrap() here should be safe.
        self.version.encode(&mut hasher).expect("blake3 hasher");
        self.msg_type.encode(&mut hasher).expect("blake3 hasher");
        self.channel.encode(&mut hasher).expect("blake3 hasher");
        self.nick.encode(&mut hasher).expect("blake3 hasher");
        self.msg.encode(&mut hasher).expect("blake3 hasher");

        hasher.finalize().into()
    }
}

// TODO: Merge Modmsg into Privmsg and distict them using type.
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
    pub mod_commands: Vec<String>,
    pub allowed_identities: Vec<PublicKey>,
    pub identity_signature_secret_key: Option<SecretKey>,
}

/// IRC contact definition
#[derive(Clone)]
pub struct IrcContact {
    /// Saltbox created for our contact public key
    pub saltbox: Arc<ChaChaBox>,
    /// Saltbox used to encrypt our nick in direct messages,
    /// created for our own public key.
    pub self_saltbox: Arc<ChaChaBox>,
}
