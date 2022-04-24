use std::net::SocketAddr;

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
    /// Raft Accept address
    #[structopt(short, long)]
    pub accept: Option<SocketAddr>,
    /// Raft Seed nodes (repeatable)
    #[structopt(short, long)]
    pub seeds: Vec<SocketAddr>,
    /// Raft Manual connection (repeatable)
    #[structopt(short, long)]
    pub connect: Vec<SocketAddr>,
    /// Raft Connection slots
    #[structopt(long, default_value = "0")]
    pub slots: u32,
    /// Increase verbosity
    #[structopt(short, parse(from_occurrences))]
    pub verbose: u8,
}
