/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

    /// Generate a new workspace
    #[structopt(long)]
    pub generate: bool,

    /// Secret Key To Encrypt/Decrypt tasks
    #[structopt(long)]
    pub workspaces: Vec<String>,

    ///  Clean all the local data in datastore path
    /// (BE CAREFUL) Check the datastore path in the config file before running this
    #[structopt(long)]
    pub refresh: bool,

    /// Current display name    
    #[structopt(long)]
    pub nickname: Option<String>,

    // Whether to pipe notifications or not
    #[structopt(long)]
    pub piped: bool,

    #[structopt(short, long)]
    /// Set log file to ouput into
    pub log: Option<String>,
}
