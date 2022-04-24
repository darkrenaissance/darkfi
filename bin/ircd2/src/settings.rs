use std::net::SocketAddr;

use serde::Deserialize;
use structopt::StructOpt;
use structopt_toml::StructOptToml;

use darkfi::net;

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
    #[structopt(long, default_value = "127.0.0.1:8857")]
    pub rpc_listen: SocketAddr,
    /// IRC listen URL
    #[structopt(long, default_value = "127.0.0.1:8855")]
    pub irc_listen: SocketAddr,
    /// Sets Datastore Path
    #[structopt(long, default_value = "~/.config/tau")]
    pub datastore: String,
    #[structopt(subcommand)]
    pub command: Command,
    /// Increase verbosity
    #[structopt(short, parse(from_occurrences))]
    pub verbose: u8,
}

#[derive(Clone, Debug, Deserialize, StructOpt)]
#[serde(tag = "type", content = "args")]
pub enum Command {
    /// Raft net settings
    Net(net::Settings),
}
