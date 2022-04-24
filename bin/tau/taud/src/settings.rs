use std::net::SocketAddr;

use darkfi::net;
use serde::Deserialize;
use structopt::StructOpt;
use structopt_toml::StructOptToml;

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
    #[structopt(long, default_value = "127.0.0.1:8857")]
    pub rpc_listen: SocketAddr,
    /// Sets Datastore Path
    #[structopt(long, default_value = "~/.config/tau")]
    pub datastore: String,
    #[structopt(subcommand)]
    pub command: Option<Command>,
    /// Increase verbosity
    #[structopt(short, parse(from_occurrences))]
    pub verbose: u8,
}

#[derive(Clone, Debug, Deserialize, StructOpt)]
#[serde(tag = "type", content = "args")]
pub enum Command {
    /// Raft net settings
    /// Note: Wihtout passing this flag, tau will work locally  
    Net(net::Settings),
}
