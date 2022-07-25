use darkfi::net::settings::SettingsOpt;
use serde::Deserialize;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

pub const CONFIG_FILE: &str = "dchat_config.toml";
pub const CONFIG_FILE_CONTENTS: &str = include_str!("../dchat_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "chatapp")]
pub struct Args {
    /// Sets a custom config file
    #[structopt(long)]
    pub config: Option<String>,

    /// Sets a custom log path
    #[structopt(long)]
    pub log_path: Option<String>,

    /// IRC listen URL
    #[structopt(long = "listen", default_value = "tcp://127.0.0.1:11066")]
    pub listen: Url,

    #[structopt(flatten)]
    pub net: SettingsOpt,

    /// Increase verbosity
    #[structopt(short, parse(from_occurrences))]
    pub verbose: u64,
}
