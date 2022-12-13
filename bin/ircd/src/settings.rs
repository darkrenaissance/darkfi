/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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
use std::collections::HashMap;

use crypto_box::SalsaBox;
use log::{info, warn};
use serde::Deserialize;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use toml::Value;
use url::Url;

use darkfi::{net::settings::SettingsOpt, Result};

// Location for config file
pub const CONFIG_FILE: &str = "ircd_config.toml";
pub const CONFIG_FILE_CONTENTS: &str = include_str!("../ircd_config.toml");

// Buffers and ordering configuration
pub const SIZE_OF_MSGS_BUFFER: usize = 4095;
pub const SIZE_OF_IDSS_BUFFER: usize = 65536;
pub const LIFETIME_FOR_ORPHAN: i64 = 600;
pub const TERM_MAX_TIME_DIFFERENCE: i64 = 180;
pub const BROADCAST_LAST_TERM_MSG: u64 = 4;

// Msg config
pub const MAXIMUM_LENGTH_OF_MESSAGE: usize = 1024;
pub const MAXIMUM_LENGTH_OF_NICKNAME: usize = 32;

// Protocol config
pub const MAX_CONFIRM: u8 = 4;
pub const UNREAD_MSG_EXPIRE_TIME: i64 = 18000;
pub const TIMEOUT_FOR_RESEND_UNREAD_MSGS: u64 = 240;

// IRC Client
pub enum RPL {
    NoTopic = 331,
    Topic = 332,
    NameReply = 353,
    EndOfNames = 366,
}

/// ircd cli
#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "ircd")]
pub struct Args {
    /// Sets a custom config file
    #[structopt(long)]
    pub config: Option<String>,

    /// JSON-RPC listen URL
    #[structopt(long = "rpc", default_value = "tcp://127.0.0.1:25550")]
    pub rpc_listen: Url,

    /// IRC listen URL
    #[structopt(long = "irc", default_value = "tcp://127.0.0.1:6667")]
    pub irc_listen: Url,

    /// Optional TLS certificate file path if `irc_listen` uses TLS
    pub irc_tls_cert: Option<String>,

    /// Optional TLS certificate key file path if `irc_listen` uses TLS
    pub irc_tls_secret: Option<String>,

    /// Generate a new NaCl secret and exit
    #[structopt(long)]
    pub gen_secret: bool,

    /// Generate a new NaCl keypair and exit
    #[structopt(long)]
    pub gen_keypair: bool,

    /// Recover public key from secret key
    #[structopt(long = "recover_pubkey")]
    pub secret: Option<String>,

    /// Path to save keypair in
    #[structopt(short)]
    pub output: Option<String>,

    /// Autojoin channels
    #[structopt(long)]
    pub autojoin: Vec<String>,

    /// Password
    #[structopt(long)]
    pub password: Option<String>,

    #[structopt(flatten)]
    pub net: SettingsOpt,

    /// Increase verbosity
    #[structopt(short, parse(from_occurrences))]
    pub verbose: u8,
}

#[derive(Clone)]
pub struct ContactInfo {
    /// Optional NaCl box for the channel, used for {en,de}cryption.
    pub salt_box: Option<SalsaBox>,
}

impl ContactInfo {
    pub fn new() -> Result<Self> {
        Ok(Self { salt_box: None })
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
#[derive(Clone)]
pub struct ChannelInfo {
    /// Optional topic for the channel
    pub topic: Option<String>,
    /// Optional NaCl box for the channel, used for {en,de}cryption.
    pub salt_box: Option<SalsaBox>,
    /// Flag indicates whether the user has joined the channel or not
    pub joined: bool,
    /// All nicknames which are visible on the channel
    pub names: Vec<String>,
}

impl ChannelInfo {
    pub fn new() -> Result<Self> {
        Ok(Self { topic: None, salt_box: None, joined: false, names: vec![] })
    }
}

fn salt_box_from_shared_secret(s: &str) -> Result<SalsaBox> {
    let bytes: [u8; 32] = bs58::decode(s).into_vec()?.try_into().unwrap();
    let secret = crypto_box::SecretKey::from(bytes);
    let public = secret.public_key();
    Ok(SalsaBox::new(&public, &secret))
}

fn parse_priv_key(data: &str) -> Result<String> {
    let mut pk = String::new();

    let map = match toml::from_str(data)? {
        Value::Table(m) => m,
        _ => return Ok(pk),
    };

    if !map.contains_key("private_key") {
        return Ok(pk)
    }

    if !map["private_key"].is_table() {
        return Ok(pk)
    }

    let private_keys = map["private_key"].as_table().unwrap();

    for prv_key in private_keys {
        pk = prv_key.0.into();
    }

    info!("Found secret key in config, noted it down.");
    Ok(pk)
}

/// Parse a TOML string for any configured contact list and return
/// a map containing said configurations.
///
/// ```toml
/// [contact."nick"]
/// contact_pubkey = "7CkVuFgwTUpJn5Sv67Q3fyEDpa28yrSeL5Hg2GqQ4jfM"
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
    let found_priv = match parse_priv_key(data) {
        Ok(v) => v,
        Err(_) => {
            info!("Did not found private key in config, skipping contact configuration.");
            return Ok(ret)
        }
    };

    let bytes: [u8; 32] = match bs58::decode(found_priv).into_vec() {
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
        if !table.contains_key("contact_pubkey") || !table["contact_pubkey"].is_str() {
            warn!("Contact {} doesn't have `contact_pubkey` set or is not a string.", cnt.0);
            continue
        }

        let pub_str = table["contact_pubkey"].as_str().unwrap();
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
        contact_info.salt_box = Some(SalsaBox::new(&public, &secret));
        ret.insert(cnt.0.to_string(), contact_info);
        info!("Instantiated NaCl box for contact {}", cnt.0);
    }

    Ok(ret)
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
                channel_info.salt_box = Some(salt_box);
                info!("Instantiated NaCl box for channel {}", chan.0);
            }
        }

        ret.insert(chan.0.to_string(), channel_info);
    }

    Ok(ret)
}

pub fn get_current_time() -> u64 {
    let start = std::time::SystemTime::now();
    start
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis()
        .try_into()
        .unwrap()
}
