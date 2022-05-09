use std::{net::SocketAddr, path::PathBuf};

use fxhash::FxHashMap;
use log::info;
use serde::Deserialize;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use toml::Value;

use darkfi::{net::settings::SettingsOpt, Result};

pub const CONFIG_FILE: &str = "ircd_config.toml";
pub const CONFIG_FILE_CONTENTS: &str = include_str!("../ircd_config.toml");

/// ircd cli
#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "ircd")]
pub struct Args {
    /// Sets a custom config file
    #[structopt(long)]
    pub config: Option<String>,

    /// JSON-RPC listen URL
    #[structopt(long = "rpc", default_value = "tcp://127.0.0.1:11055")]
    pub rpc_listen: String,

    /// IRC listen URL
    #[structopt(long = "irc", default_value = "127.0.0.1:11066")]
    pub irc_listen: SocketAddr,

    /// Sets Datastore Path
    #[structopt(long, default_value = "~/.config/ircd")]
    pub datastore: String,

    /// Generate a new NaCl secret and exit
    #[structopt(long)]
    pub gen_secret: bool,

    /// Autojoin channels
    #[structopt(long)]
    pub autojoin: Vec<String>,

    #[structopt(flatten)]
    pub net: SettingsOpt,

    /// Increase verbosity
    #[structopt(short, parse(from_occurrences))]
    pub verbose: u8,
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
    pub salt_box: Option<crypto_box::Box>,
}

impl ChannelInfo {
    pub fn new() -> Result<Self> {
        Ok(Self { topic: None, salt_box: None })
    }
}

/// Parse the configuration file for any configured channels and return
/// a map containing said configurations.
pub fn parse_configured_channels(config_file: &PathBuf) -> Result<FxHashMap<String, ChannelInfo>> {
    let toml_contents = std::fs::read_to_string(config_file)?;
    let mut ret = FxHashMap::default();

    match toml::from_str(&toml_contents)? {
        Value::Table(map) => {
            if map.contains_key("channel") && map["channel"].is_table() {
                for chan in map["channel"].as_table().unwrap() {
                    info!("Found configuration for channel {}", chan.0);
                    let mut channel_info = ChannelInfo::new()?;

                    if chan.1.as_table().unwrap().contains_key("topic") {
                        channel_info.topic = Some(chan.1["topic"].as_str().unwrap().to_string());
                    }

                    if chan.1.as_table().unwrap().contains_key("secret") {
                        // Build the NaCl box
                        let s = chan.1["secret"].as_str().unwrap();
                        let bytes: [u8; 32] = bs58::decode(s).into_vec()?.try_into().unwrap();
                        let secret = crypto_box::SecretKey::from(bytes);
                        let public = secret.public_key();
                        let msg_box = crypto_box::Box::new(&public, &secret);
                        channel_info.salt_box = Some(msg_box);
                    }

                    ret.insert(chan.0.to_string(), channel_info);
                }
            }
        }
        _ => {}
    };

    Ok(ret)
}
