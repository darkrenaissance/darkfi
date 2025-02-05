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

use std::sync::Arc;

use async_std::prelude::StreamExt;
use log::info;
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

use darkfi::{async_daemonize, cli_desc, Result};

use rlnd::Rlnd;

const CONFIG_FILE: &str = "rlnd_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../rlnd_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "rlnd", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(long, default_value = "~/.local/share/darkfi/rlnd")]
    /// Path to the database directory
    database: String,

    #[structopt(short, long, default_value = "tcp://127.0.0.1:25637")]
    /// Private JSON-RPC listen URL
    private_rpc_listen: Url,

    #[structopt(short, long, default_value = "tcp://127.0.0.1:25638")]
    /// Publicly exposed JSON-RPC listen URL
    public_rpc_listen: Url,

    #[structopt(short, long, default_value = "tcp://127.0.0.1:26660")]
    /// darkirc JSON-RPC endpoint
    endpoint: Url,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    info!(target: "rlnd", "Initializing DarkFi RLN state management node...");

    // Generate the daemon
    let daemon = Rlnd::init(&args.database, &args.endpoint, &ex).await?;

    // Start the daemon
    daemon.start(&ex, &args.private_rpc_listen, &args.public_rpc_listen).await?;

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "rlnd", "Caught termination signal, cleaning up and exiting...");

    daemon.stop().await?;

    info!(target: "rlnd", "Shut down successfully");

    Ok(())
}
