use std::collections::HashSet;

use darkfi::{system::StoppableTaskPtr, Result};
use serde::Deserialize;
use smol::lock::Mutex;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[structopt()]
pub struct SwapdArgs {
    #[structopt(long, default_value = "tcp://127.0.0.1:52821")]
    /// darkfi-swapd JSON-RPC listen URL
    pub swapd_rpc: Url,

    #[structopt(long, default_value = "~/.local/darkfi/swapd")]
    /// Path to swapd's filesystem database
    pub swapd_db: String,
}

/// Swapd daemon state
pub struct Swapd {
    /// Main reference to the swapd filesystem databaase
    _sled_db: sled::Db,
    /// JSON-RPC connection tracker
    pub(crate) rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
}

impl Swapd {
    /// Instantiate `Swapd` state
    pub async fn new(_swapd_args: &SwapdArgs, sled_db: sled::Db) -> Result<Self> {
        Ok(Self { _sled_db: sled_db, rpc_connections: Mutex::new(HashSet::new()) })
    }
}
