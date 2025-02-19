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

use log::info;
use smol::{stream::StreamExt, Executor};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};

use darkfi::{async_daemonize, cli_desc, rpc::settings::RpcSettingsOpt, Result};

use minerd::Minerd;

const CONFIG_FILE: &str = "minerd.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../minerd.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "minerd", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(flatten)]
    /// JSON-RPC settings
    rpc: RpcSettingsOpt,

    #[structopt(short, long, default_value = "4")]
    /// PoW miner number of threads to use
    threads: usize,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'static>>) -> Result<()> {
    info!(target: "minerd", "Starting DarkFi Mining Daemon...");
    let daemon = Minerd::init(args.threads);
    daemon.start(&ex, &args.rpc.into());

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "minerd", "Caught termination signal, cleaning up and exiting");

    daemon.stop().await?;

    info!(target: "minerd", "Shut down successfully");
    Ok(())
}
