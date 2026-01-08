/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use darkfi::{
    cli_desc, dht::DhtSettingsOpt, net::settings::SettingsOpt, rpc::settings::RpcSettingsOpt,
};

use crate::pow::PowSettingsOpt;

pub const CONFIG_FILE: &str = "fud_config.toml";
pub const CONFIG_FILE_CONTENTS: &str = include_str!("../fud_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "fud", about = cli_desc!())]
pub struct Args {
    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    pub verbose: u8,

    #[structopt(short, long)]
    /// Configuration file to use
    pub config: Option<String>,

    #[structopt(long)]
    /// Set log file path to output daemon logs into
    pub log: Option<String>,

    #[structopt(long, default_value = "~/.local/share/darkfi/fud")]
    /// Base directory for filesystem storage
    pub base_dir: String,

    #[structopt(short, long)]
    /// Default path to store downloaded files (defaults to <base_dir>/downloads)
    pub downloads_path: Option<String>,

    #[structopt(long, default_value = "60")]
    /// Chunk transfer timeout in seconds
    pub chunk_timeout: u64,

    #[structopt(flatten)]
    /// Network settings
    pub net: SettingsOpt,

    #[structopt(skip)]
    /// Main JSON-RPC settings
    pub rpc: RpcSettingsOpt,

    #[structopt(skip)]
    /// Management JSON-RPC settings
    pub management_rpc: RpcSettingsOpt,

    #[structopt(flatten)]
    /// DHT settings
    pub dht: DhtSettingsOpt,

    #[structopt(flatten)]
    /// PoW settings
    pub pow: PowSettingsOpt,
}
