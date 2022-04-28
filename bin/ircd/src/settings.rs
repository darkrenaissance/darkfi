use std::net::SocketAddr;

use serde::Deserialize;
use structopt::StructOpt;
use structopt_toml::StructOptToml;

use darkfi::net::settings::SettingsOpt;

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
    #[structopt(long = "rpc", default_value = "127.0.0.1:11055")]
    pub rpc_listen: SocketAddr,
    /// IRC listen URL
    #[structopt(long = "irc", default_value = "127.0.0.1:11066")]
    pub irc_listen: SocketAddr,
    /// Sets Datastore Path
    #[structopt(long, default_value = "~/.config/ircd")]
    pub datastore: String,
    #[structopt(flatten)]
    pub net: SettingsOpt,
    /// Increase verbosity
    #[structopt(short, parse(from_occurrences))]
    pub verbose: u8,
}
