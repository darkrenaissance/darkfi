use crypto_box::SalsaBox;
use fxhash::FxHashMap;
use log::info;
use serde::Deserialize;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use toml::Value;
use url::Url;

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
    if let Value::Table(map) = toml::from_str(data)? {
        if map.contains_key("private_key") && map["private_key"].is_table() {
            for prv_key in map["private_key"].as_table().unwrap() {
                pk = prv_key.0.into();
            }
        }
    };

    Ok(pk)
}


/// Parse a TOML string for any configured contact list and return
/// a map containing said configurations.
///
/// ```toml
/// [contact."nick"]
/// contact_pubkey = "7CkVuFgwTUpJn5Sv67Q3fyEDpa28yrSeL5Hg2GqQ4jfM"
/// ```
pub fn parse_configured_contacts(data: &str) -> Result<FxHashMap<String, ContactInfo>> {
    let mut ret = FxHashMap::default();

    if let Value::Table(map) = toml::from_str(data)? {
        if map.contains_key("contact") && map["contact"].is_table() {
            for cnt in map["contact"].as_table().unwrap() {
                info!("Found configuration for contact {}", cnt.0);

                let mut contact_info = ContactInfo::new()?;

                if cnt.1.as_table().unwrap().contains_key("contact_pubkey") {
                    // Build the NaCl box
                    if let Some(p) = cnt.1["contact_pubkey"].as_str() {
                        let bytes: [u8; 32] = bs58::decode(p).into_vec()?.try_into().unwrap();
                        let public = crypto_box::PublicKey::from(bytes);

                        let bytes: [u8; 32] =
                            bs58::decode(parse_priv_key(data)?).into_vec()?.try_into().unwrap();
                        let secret = crypto_box::SecretKey::from(bytes);

                        contact_info.salt_box = Some(SalsaBox::new(&public, &secret));

                        ret.insert(cnt.0.to_string(), contact_info);
                        info!("Instantiated NaCl box for contact {}", cnt.0);
                    }
                }
            }
        }
    };

    Ok(ret)
}

/// TODO CLEAN UP
/// Parse a TOML string for any configured channels and return
/// a map containing said configurations.
///
/// ```toml
/// [channel."#memes"]
/// secret = "7CkVuFgwTUpJn5Sv67Q3fyEDpa28yrSeL5Hg2GqQ4jfM"
/// topic = "Dank Memes"
/// ```
pub fn parse_configured_channels(data: &str) -> Result<FxHashMap<String, ChannelInfo>> {
    let mut ret = FxHashMap::default();

    if let Value::Table(map) = toml::from_str(data)? {
        if map.contains_key("channel") && map["channel"].is_table() {
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
        }
    };

    Ok(ret)
}
