use serde::Deserialize;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

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
    #[structopt(long = "rpc", default_value = "tcp://127.0.0.1:23330")]
    pub rpc_listen: Url,
    /// Sets Datastore Path
    #[structopt(long, default_value = "~/.tau")]
    pub datastore: String,
    #[structopt(flatten)]
    pub net: SettingsOpt,
    /// Increase verbosity
    #[structopt(short, parse(from_occurrences))]
    pub verbose: u8,
    /// Generate a new secret key
    #[structopt(long)]
    pub key_gen: bool,
    ///  Clean all the local data in datastore path
    /// (BE CAREFULL) Check the datastore path in the config file before running this
    #[structopt(long)]
    pub refresh: bool,
    /// Current display name    
    #[structopt(long)]
    pub nickname: Option<String>,
}
