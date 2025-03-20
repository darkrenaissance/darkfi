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
use darkfi_sdk::crypto::{schnorr::SchnorrSecret, PublicKey, SecretKey};
use darkfi_serial::{async_trait, serialize, Encodable, SerialDecodable, SerialEncodable};

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

/// Hardcoded current 'Msg' version
const MSG_VERSION: u8 = 0;

/// 'Msg' types
const MSG_TYPE_PRIV: u8 = 0;
const MSG_TYPE_MOD: u8 = 1;

/// Message structure definition.
/// Based on the type, each field represents different information.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct Msg {
    pub version: u8,
    pub msg_type: u8,
    /// Channel this message is for
    pub channel: String,
    /// Message sender nick.
    /// For `PRIVMSG_TYPE_MOD` this represents the moderation command.
    pub nick: String,
    /// The actual message.
    /// For `PRIVMSG_TYPE_MOD` this represents the command parameters.
    pub msg: String,
    /// Optional signature bytes of the message hash,
    /// for validating identity/moderator.
    pub signature: Vec<u8>,
}

impl Msg {
    /// Generate a new `Msg` of `MSG_TYPE_PRIV` type and sign it if a secret key is provided.
    pub fn new_priv(
        channel: String,
        nick: String,
        msg: String,
        secret_key: Option<SecretKey>,
    ) -> Self {
        let mut message = Self {
            version: MSG_VERSION,
            msg_type: MSG_TYPE_PRIV,
            channel,
            nick,
            msg,
            signature: vec![],
        };
        if let Some(secret_key) = secret_key {
            message.signature = serialize(&secret_key.sign(&message.hash()));
        }
        message
    }

    /// Generate a new `Msg` of `MSG_TYPE_MOD` type and sign it using provided secret key.
    pub fn new_mod(channel: String, nick: String, msg: String, secret_key: &SecretKey) -> Self {
        let mut message = Self {
            version: MSG_VERSION,
            msg_type: MSG_TYPE_MOD,
            channel,
            nick,
            msg,
            signature: vec![],
        };
        message.signature = serialize(&secret_key.sign(&message.hash()));
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
