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

use crypto_box::SalsaBox;
use log::error;
use serde::{self, Deserialize, Serialize};
use std::collections::HashMap;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

use darkfi::{net::settings::SettingsOpt, Result};

// Location for config file
pub const CONFIG_FILE: &str = "ircd_config.toml";
pub const CONFIG_FILE_CONTENTS: &str = include_str!("../ircd_config.toml");

// Msg config
pub const MAXIMUM_LENGTH_OF_MESSAGE: usize = 1024;
pub const MAXIMUM_LENGTH_OF_NICKNAME: usize = 32;

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

    /// Generate a new NaCl keypair and exit
    #[structopt(long)]
    pub gen_keypair: bool,

    /// Path to save keypair in
    #[structopt(short)]
    pub output: Option<String>,

    /// Autojoin channels
    #[structopt(long)]
    pub autojoin: Vec<String>,

    /// Password
    #[structopt(long)]
    pub password: Option<String>,

    /// Channels
    #[structopt(skip)]
    pub channels: HashMap<String, ChannelInfo>,

    /// Channels
    #[structopt(skip)]
    pub contacts: HashMap<String, ContactInfo>,

    /// Private key
    #[structopt(skip)]
    pub private_key: Option<String>,

    #[structopt(flatten)]
    pub net: SettingsOpt,

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
#[derive(Default, Clone, Debug, Deserialize, Serialize)]
pub struct ContactInfo {
    pub pubkey: Option<String>,
}

impl ContactInfo {
    pub fn new() -> Self {
        Self { pubkey: None }
    }

    pub fn salt_box(&self, private_key: &str, contact_name: &str) -> Option<SalsaBox> {
        if let Ok(private) = parse_priv(private_key) {
            if let Some(p) = &self.pubkey {
                if let Ok(public) = parse_pub(p) {
                    return Some(SalsaBox::new(&public, &private))
                } else {
                    error!("Uncorrect public key in for contact {}", contact_name);
                }
            }
        } else {
            error!("Uncorrect Private key in config",);
        }

        None
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
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct ChannelInfo {
    /// Optional topic for the channel
    pub topic: Option<String>,
    /// Optional NaCl box for the channel, used for {en,de}cryption.
    pub secret: Option<String>,
    /// Flag indicates whether the user has joined the channel or not
    #[serde(default, skip_serializing)]
    pub joined: bool,
    /// All nicknames which are visible on the channel
    #[serde(default, skip_serializing)]
    pub names: Vec<String>,
}

impl ChannelInfo {
    pub fn new() -> Self {
        Self { topic: None, secret: None, joined: false, names: vec![] }
    }

    pub fn salt_box(&self, channel_name: &str) -> Option<SalsaBox> {
        if let Some(s) = &self.secret {
            let secret = parse_priv(s);

            if secret.is_err() {
                error!("Uncorrect secret key for the channel {}", channel_name);
                return None
            }

            let secret = secret.unwrap();
            let public = secret.public_key();
            return Some(SalsaBox::new(&public, &secret))
        }
        None
    }
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

fn parse_priv(key: &str) -> Result<crypto_box::SecretKey> {
    let bytes: [u8; 32] = bs58::decode(key).into_vec()?.try_into().unwrap();
    Ok(crypto_box::SecretKey::from(bytes))
}

fn parse_pub(key: &str) -> Result<crypto_box::PublicKey> {
    let bytes: [u8; 32] = bs58::decode(key).into_vec()?.try_into().unwrap();
    Ok(crypto_box::PublicKey::from(bytes))
}
