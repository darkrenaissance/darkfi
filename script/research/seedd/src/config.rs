use serde_derive::Deserialize;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

use darkfi::cli_desc;

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "seedd", about = cli_desc!())]
pub struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    pub config: Option<String>,

    #[structopt(long, default_value = "tcp://127.0.0.1")]
    /// Daemon published url, common for all enabled networks
    pub url: Url,

    #[structopt(long)]
    /// Darkfid activation flag
    pub darkfid: bool,

    #[structopt(flatten)]
    /// Darkfid network specific settings
    pub darkfid_opts: NetOpt,

    #[structopt(long)]
    /// Ircd activation flag
    pub ircd: bool,

    #[structopt(flatten)]
    /// Ircd network specific settings
    pub ircd_opts: NetOpt,

    #[structopt(long)]
    /// Taud activation flag
    pub taud: bool,

    #[structopt(flatten)]
    /// Taud network specific settings
    pub taud_opts: NetOpt,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    pub verbose: u8,
}

/// Defines the network specific settings
#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[structopt()]
pub struct NetOpt {
    #[structopt(skip)]
    /// Specific port the network will use
    pub port: u16,

    #[structopt(skip)]
    /// Connect to seeds (repeatable flag)
    pub seeds: Vec<Url>,

    #[structopt(skip)]
    /// Connect to peers (repeatable flag)
    pub peers: Vec<Url>,
}
