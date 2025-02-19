/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use structopt::StructOpt;
use structopt_toml::{serde::Deserialize, StructOptToml};

use darkfi::{net::settings::SettingsOpt, rpc::settings::RpcSettingsOpt};

pub const CONFIG_FILE: &str = "taud_config.toml";
pub const CONFIG_FILE_CONTENTS: &str = include_str!("../taud_config.toml");

/// taud cli
#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "taud")]
pub struct Args {
    #[structopt(long)]
    /// Sets a custom config file
    pub config: Option<String>,

    #[structopt(long, default_value = "~/.local/share/darkfi/taud_db")]
    /// Sets Datastore Path
    pub datastore: String,

    #[structopt(long, default_value = "~/.local/share/darkfi/replayed_taud_db")]
    /// Replay logs (DB) path
    pub replay_datastore: String,

    #[structopt(long)]
    /// Flag to store Sled DB instructions
    pub replay_mode: bool,

    #[structopt(flatten)]
    /// JSON-RPC settings
    pub rpc: RpcSettingsOpt,

    #[structopt(flatten)]
    /// P2P network settings
    pub net: SettingsOpt,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity
    pub verbose: u8,

    #[structopt(long)]
    /// Generate a new workspace
    pub generate: bool,

    #[structopt(long)]
    /// Secret Key To Encrypt/Decrypt tasks
    pub workspaces: Vec<String>,

    #[structopt(long)]
    /// Write access key
    pub write: Option<String>,

    #[structopt(long)]
    /// Password
    pub password: Option<String>,

    #[structopt(long)]
    /// Clean all the local data in datastore path
    /// (BE CAREFUL) Check the datastore path in the config file before running this
    pub refresh: bool,

    #[structopt(long)]
    /// Current display name
    pub nickname: Option<String>,

    #[structopt(long)]
    /// Flag to skip syncing the DAG (no history)
    pub skip_dag_sync: bool,

    #[structopt(long, default_value = "/tmp/tau_pipe")]
    /// Named pipe path
    pub pipe_path: String,

    #[structopt(long)]
    // Whether to pipe notifications or not
    pub piped: bool,

    #[structopt(short, long)]
    /// Set log file to ouput into
    pub log: Option<String>,
}
