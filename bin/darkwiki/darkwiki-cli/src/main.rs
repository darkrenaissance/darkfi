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

use serde_json::json;
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use structopt::StructOpt;
use url::Url;

use darkfi::{
    rpc::{client::RpcClient, jsonrpc::JsonRequest},
    util::cli::{get_log_config, get_log_level},
    Result,
};

#[derive(Clone, Debug, StructOpt)]
#[structopt(name = "darkwikiupdate")]
struct Args {
    #[structopt(subcommand)]
    sub_command: ArgsSubCommand,
    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
    #[structopt(short, long, default_value = "tcp://127.0.0.1:24330")]
    /// darkfid JSON-RPC endpoint
    endpoint: Url,
}

#[derive(Debug, Clone, PartialEq, StructOpt)]
enum ArgsSubCommand {
    /// Publish local patches and merging received patches
    Update {
        #[structopt(long, short)]
        /// Run without applying the changes
        dry_run: bool,
        /// Names of files to update (Note: Will update all the documents if left empty)
        values: Vec<String>,
    },
    /// Show the history of patches  
    Log {
        /// Names of files to log (Note: Will show all the log if left empty)
        values: Vec<String>,
    },
    /// Undo the local changes
    Restore {
        #[structopt(long, short)]
        /// Run without applying the changes
        dry_run: bool,
        /// Names of files to restore (Note: Will restore all the documents if left empty)
        values: Vec<String>,
    },
}

fn print_patches(value: &Vec<serde_json::Value>) {
    for res in value {
        let res = res.as_array().unwrap();
        let res: Vec<&str> = res.iter().map(|r| r.as_str().unwrap()).collect();
        let (title, workspace, changes) = (res[0], res[1], res[2]);
        println!("WORKSPACE: {} FILE: {}", workspace, title);
        println!("{}", changes);
        println!("----------------------------------");
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = Args::from_args();

    let log_level = get_log_level(args.verbose.into());
    let log_config = get_log_config();
    TermLogger::init(log_level, log_config, TerminalMode::Mixed, ColorChoice::Auto)?;

    let rpc_client = RpcClient::new(args.endpoint).await?;

    match args.sub_command {
        ArgsSubCommand::Update { dry_run, values } => {
            let req = JsonRequest::new("update", json!([dry_run, values]));

            let result = rpc_client.request(req).await?;

            let result = result.as_array().unwrap();
            let local_patches = result[0].as_array().unwrap();
            let sync_patches = result[1].as_array().unwrap();
            let merge_patches = result[2].as_array().unwrap();

            if !local_patches.is_empty() {
                println!();
                println!("PUBLISH LOCAL PATCHES:");
                println!();
                print_patches(local_patches);
            }

            if !sync_patches.is_empty() {
                println!();
                println!("RECEIVED PATCHES:");
                println!();
                print_patches(sync_patches);
            }

            if !merge_patches.is_empty() {
                println!();
                println!("MERGE:");
                println!();
                print_patches(merge_patches);
            }
        }
        ArgsSubCommand::Restore { dry_run, values } => {
            let req = JsonRequest::new("restore", json!([dry_run, values]));
            let result = rpc_client.request(req).await?;

            let result = result.as_array().unwrap();
            let patches = result[0].as_array().unwrap();

            if !patches.is_empty() {
                println!();
                println!("AFTER RESTORE:");
                println!();
                print_patches(patches);
            }
        }
        _ => unimplemented!(),
    }

    rpc_client.close().await
}
