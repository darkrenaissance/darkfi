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

use darkfi::cli_desc;

#[derive(clap::Parser)]
#[clap(name = "dnetview", about = cli_desc!(), version)]
pub struct Args {
    #[clap(short, action = clap::ArgAction::Count)]
    /// Increase verbosity (-vvv supported)
    pub verbose: u8,

    /// Logfile path
    #[clap(default_value = "~/.local/darkfi/dnetview.log")]
    pub log_path: String,

    /// Sets a custom config file
    #[clap(short, long)]
    pub config: Option<String>,
}
