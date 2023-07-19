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

use async_std::sync::Arc;
use crypto_box::ChaChaBox;
use log::{info, warn};
use serde::{self, Deserialize};
use std::collections::{HashMap, HashSet};
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use toml::Value;
use url::Url;

use darkfi::{net::settings::SettingsOpt, Result};

// Location for config file
pub const CONFIG_FILE: &str = "darkirc_config.toml";
pub const CONFIG_FILE_CONTENTS: &str = include_str!("../darkirc_config.toml");

// Msg config
pub const MAXIMUM_LENGTH_OF_MESSAGE: usize = 1024;
pub const MAXIMUM_LENGTH_OF_NICK_CHAN_CNT: usize = 32;

// IRC Client
pub enum RPL {
    NoTopic = 331,
    Topic = 332,
    NameReply = 353,
    EndOfNames = 366,
}

/// ircd cli
#[derive(Clone, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "darkirc")]
pub struct Args {
    /// Sets a custom config file
    #[structopt(long)]
    pub config: Option<String>,

    /// JSON-RPC listen URL
    #[structopt(long = "rpc", default_value = "tcp://127.0.0.1:26660")]
    pub rpc_listen: Url,

    /// IRC listen URL
    #[structopt(long = "irc", default_value = "tcp://127.0.0.1:6667")]
    pub irc_listen: Url,

    /// Optional TLS certificate file path if `irc_listen` uses TLS
    pub irc_tls_cert: Option<String>,

    /// Optional TLS certificate key file path if `irc_listen` uses TLS
    pub irc_tls_secret: Option<String>,

    /// Generate a new NaCl keypair and exit
    #[structopt(long)]
    pub gen_keypair: bool,

    /// Generate a new NaCl secret for an encrypted channel and exit
    #[structopt(long)]
    pub gen_secret: bool,

    /// Path to save keypair in
    #[structopt(short)]
    pub output: Option<String>,

    /// Autojoin channels
    #[structopt(long)]
    pub autojoin: Vec<String>,

    /// Password
    #[structopt(long)]
    pub password: Option<String>,

    /// Network settings
    #[structopt(flatten)]
    pub net: SettingsOpt,

    #[structopt(short, long)]
    /// Set log file to ouput into
    pub log: Option<String>,

    /// Increase verbosity
    #[structopt(short, parse(from_occurrences))]
    pub verbose: u8,
}

/// This struct holds information about preconfigured contacts.
/// In the TOML configuration file, we can configure contacts as such:
///
/// ```toml
/// [contact."nick"]
/// pubkey = "7CkVuFgwTUpJn5Sv67Q3fyEDpa28yrSeL5Hg2GqQ4jfM"
/// ```
#[derive(Clone)]
pub struct ContactInfo {
    /// Optional NaCl box for the channel, used for {en,de}cryption.
    pub salt_box: Option<Arc<ChaChaBox>>,
}

impl ContactInfo {
    pub fn new() -> Result<Self> {
        Ok(Self { salt_box: None })
    }
}

/// Defined user modes
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum UserMode {
    None,
    Op,
    Voice,
    HalfOp,
    Admin,
    Owner,
}

impl std::fmt::Display for UserMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        match self {
            Self::None => write!(f, ""),
            Self::Op => write!(f, "@"),
            Self::Voice => write!(f, "+"),
            Self::HalfOp => write!(f, "%"),
            Self::Admin => write!(f, "&"),
            Self::Owner => write!(f, "~"),
        }
    }
}

/// This struct holds info about a specific nickname within a channel.
/// We usually use it to implement modes.
#[derive(Debug, Clone, Eq)]
pub struct Nick {
    name: String,
    mode: UserMode,
}

impl Nick {
    pub fn new(name: String) -> Self {
        Self { name, mode: UserMode::None }
    }

    pub fn set_mode(&mut self, mode: UserMode) -> Option<String> {
        if self.mode == mode {
            return None
        }

        self.mode = mode;
        Some(format!("+{}", mode))
    }

    pub fn unset_mode(&mut self, mode: UserMode) -> Option<String> {
        if self.mode != mode {
            return None
        }

        self.mode = mode;
        Some(format!("-{}", mode))
    }
}

impl PartialEq for Nick {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl From<String> for Nick {
    fn from(name: String) -> Self {
        Self { name, mode: UserMode::None }
    }
}

impl std::hash::Hash for Nick {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        state.write(&self.name.clone().into_bytes());
    }
}

impl std::fmt::Display for Nick {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        write!(f, "{}{}", self.mode, self.name)
    }
}

/// This struct holds information about preconfigured channels.
/// In the TOML configuration file, we can configure channels as such:
/// ```toml
/// [channel."#dev"]
/// secret = "GvH4kno3kUu6dqPrZ8zjMhqxTUDZ2ev16EdprZiZJgj1"
/// topic = "DarkFi Development Channel"
/// ```
/// Having a secret will enable a NaCl box that is able to encrypt and
/// decrypt messages in this channel using this set shared secret.
/// The secret should be shared OOB, via a secure channel.
/// Having a topic set is useful if one wants to have a topic in the
/// configured channel. It is not shared with others, but it is useful
/// for personal reference.
#[derive(Default, Clone)]
pub struct ChannelInfo {
    /// Optional topic for the channel
    pub topic: Option<String>,
    /// Optional NaCl box for the channel, used for {en,de}cryption.
    pub salt_box: Option<Arc<ChaChaBox>>,
    /// Flag indicates whether the user has joined the channel or not
    pub joined: bool,
    /// All nicknames which are visible on the channel
    pub names: HashSet<Nick>,
}

impl ChannelInfo {
    pub fn new() -> Result<Self> {
        Ok(Self { topic: None, salt_box: None, joined: false, names: HashSet::new() })
    }

    pub fn names(&self) -> String {
        self.names.iter().map(|n| n.to_string()).collect::<Vec<String>>().join(" ")
    }
}

/// Parse a TOML string for any configured channels and return
/// a map containing said configurations.
///
/// ```toml
/// [channel."#memes"]
/// secret = "7CkVuFgwTUpJn5Sv67Q3fyEDpa28yrSeL5Hg2GqQ4jfM"
/// topic = "Dank Memes"
/// ```
pub fn parse_configured_channels(data: &str) -> Result<HashMap<String, ChannelInfo>> {
    let mut ret = HashMap::new();

    let map = match toml::from_str(data)? {
        Value::Table(m) => m,
        _ => return Ok(ret),
    };

    if !map.contains_key("channel") {
        return Ok(ret)
    }

    if !map["channel"].is_table() {
        return Ok(ret)
    }

    for chan in map["channel"].as_table().unwrap() {
        if chan.0.len() > MAXIMUM_LENGTH_OF_NICK_CHAN_CNT {
            warn!("Channel name is too long, skipping...");
            continue
        }
        info!("Found configuration for channel {}", chan.0);
        let mut channel_info = ChannelInfo::new()?;

        if chan.1.as_table().unwrap().contains_key("topic") {
            let topic = chan.1["topic"].as_str().unwrap().to_string();
            info!("Found topic for channel {}: {}", chan.0, topic);
            channel_info.topic = Some(topic);
        }

        if chan.1.as_table().unwrap().contains_key("secret") {
            // Build the NaCl box
            if let Some(s) = chan.1["secret"].as_str() {
                let salt_box = salt_box_from_shared_secret(s)?;
                channel_info.salt_box = Some(Arc::new(salt_box));
                info!("Instantiated NaCl box for channel {}", chan.0);
            }
        }

        ret.insert(chan.0.to_string(), channel_info);
    }

    Ok(ret)
}

/// Parse a TOML string for any configured contact list and return
/// a map containing said configurations.
///
/// ```toml
/// [contact."nick"]
/// public_key = "7CkVuFgwTUpJn5Sv67Q3fyEDpa28yrSeL5Hg2GqQ4jfM"
/// ```
pub fn parse_configured_contacts(data: &str) -> Result<HashMap<String, ContactInfo>> {
    let mut ret = HashMap::new();

    let map = match toml::from_str(data) {
        Ok(Value::Table(m)) => m,
        _ => {
            warn!("Invalid TOML string passed as argument to parse_configured_contacts()");
            return Ok(ret)
        }
    };

    if !map.contains_key("contact") {
        return Ok(ret)
    }

    if !map["contact"].is_table() {
        warn!("TOML configuration contains a \"contact\" field, but it is not a table.");
        return Ok(ret)
    }

    let contacts = map["contact"].as_table().unwrap();

    // Our secret key for NaCl boxes.
    let found_secret = match parse_secret_key(data) {
        Ok(v) => v,
        Err(_) => {
            info!("Did not find secret key in config, skipping contact configuration.");
            return Ok(ret)
        }
    };

    let bytes: [u8; 32] = match bs58::decode(found_secret).into_vec() {
        Ok(v) => {
            if v.len() != 32 {
                warn!("Decoded base58 secret key string is not 32 bytes");
                warn!("Skipping private contact configuration");
                return Ok(ret)
            }
            v.try_into().unwrap()
        }
        Err(e) => {
            warn!("Failed to decode base58 secret key from string: {}", e);
            warn!("Skipping private contact configuration");
            return Ok(ret)
        }
    };

    let secret = crypto_box::SecretKey::from(bytes);

    for cnt in contacts {
        if cnt.0.len() > MAXIMUM_LENGTH_OF_NICK_CHAN_CNT {
            warn!("Contact name is too long, skipping...");
            continue
        }
        info!("Found configuration for contact {}", cnt.0);
        let mut contact_info = ContactInfo::new()?;

        if !cnt.1.is_table() {
            warn!("Config for contact {} isn't a TOML table", cnt.0);
            continue
        }

        let table = cnt.1.as_table().unwrap();
        if table.is_empty() {
            warn!("Configuration for contact {} is empty.", cnt.0);
            continue
        }

        // Build the NaCl box
        if !table.contains_key("public_key") || !table["public_key"].is_str() {
            warn!("Contact {} doesn't have `public_key` set or is not a valid string.", cnt.0);
            continue
        }

        let pub_str = table["public_key"].as_str().unwrap();
        let bytes: [u8; 32] = match bs58::decode(pub_str).into_vec() {
            Ok(v) => {
                if v.len() != 32 {
                    warn!("Decoded base58 string is not 32 bytes");
                    continue
                }

                v.try_into().unwrap()
            }
            Err(e) => {
                warn!("Failed to decode base58 pubkey from string: {}", e);
                continue
            }
        };

        let public = crypto_box::PublicKey::from(bytes);
        contact_info.salt_box = Some(Arc::new(ChaChaBox::new(&public, &secret)));
        ret.insert(cnt.0.to_string(), contact_info);
        info!("Instantiated NaCl box for contact \"{}\"", cnt.0);
    }

    Ok(ret)
}

fn salt_box_from_shared_secret(s: &str) -> Result<ChaChaBox> {
    let bytes: [u8; 32] = bs58::decode(s).into_vec()?.try_into().unwrap();
    let secret = crypto_box::SecretKey::from(bytes);
    let public = secret.public_key();
    Ok(ChaChaBox::new(&public, &secret))
}

fn parse_secret_key(data: &str) -> Result<String> {
    let mut sk = String::new();

    let map = match toml::from_str(data)? {
        Value::Table(m) => m,
        _ => return Ok(sk),
    };

    if !map.contains_key("secret_key") {
        return Ok(sk)
    }

    if !map["secret_key"].is_table() {
        return Ok(sk)
    }

    let secret_keys = map["secret_key"].as_table().unwrap();

    for key in secret_keys {
        sk = key.0.into();
    }

    info!("Found secret key in config, noted it down.");
    Ok(sk)
}
