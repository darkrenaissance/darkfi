use serde::Deserialize;
use structopt::StructOpt;
use structopt_toml::StructOptToml;

use darkfi::net::settings::SettingsOpt;

pub const CONFIG_FILE: &str = "taud_config.toml";
pub const CONFIG_FILE_CONTENTS: &str = include_str!("../../taud_config.toml");

/// taud cli
#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "taud")]
pub struct Args {
    /// Sets a custom config file
    #[structopt(long)]
    pub config: Option<String>,
    /// JSON-RPC listen URL
    #[structopt(long = "rpc", default_value = "tcp://127.0.0.1:11055")]
    pub rpc_listen: String,
    /// Sets Datastore Path
    #[structopt(long, default_value = "~/.config/tau")]
    pub datastore: String,
    #[structopt(flatten)]
    pub net: SettingsOpt,
    /// Increase verbosity
    #[structopt(short, parse(from_occurrences))]
    pub verbose: u8,
    /// Generate a new secret key
    #[structopt(long)]
    pub key_gen: bool,
    /// Current display name    
    #[structopt(long)]
    pub nickname: Option<String>,
}
