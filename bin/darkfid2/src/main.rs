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

use async_std::{stream::StreamExt, sync::Arc};
use log::info;
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};

use darkfi::{
    async_daemonize,
    blockchain::BlockInfo,
    cli_desc,
    util::time::TimeKeeper,
    validator::{Validator, ValidatorConfig, ValidatorPtr},
    Result,
};
use darkfi_contract_test_harness::vks;

#[cfg(test)]
mod tests;

const CONFIG_FILE: &str = "darkfid_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../darkfid_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "darkfid", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(long)]
    /// Enable testing mode for local testing
    testing_mode: bool,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

pub struct Darkfid {
    _validator: ValidatorPtr,
}

impl Darkfid {
    pub async fn new(_validator: ValidatorPtr) -> Self {
        Self { _validator }
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, _ex: Arc<smol::Executor<'_>>) -> Result<()> {
    info!("Initializing DarkFi node...");

    // NOTE: everything is dummy for now
    // Initialize or open sled database
    let sled_db = sled::Config::new().temporary(true).open()?;
    vks::inject(&sled_db)?;

    // Initialize validator configuration
    let genesis_block = BlockInfo::default();
    let time_keeper = TimeKeeper::new(genesis_block.header.timestamp, 10, 90, 0);
    let config = ValidatorConfig::new(time_keeper, genesis_block, vec![], args.testing_mode);

    if args.testing_mode {
        info!("Node is configured to run in testing mode!");
    }

    // Initialize validator
    let validator = Validator::new(&sled_db, config).await?;

    // Initialize node
    let _darkfid = Darkfid::new(validator).await;
    info!("Node initialized successfully!");

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new()?;
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting...");

    Ok(())
}
