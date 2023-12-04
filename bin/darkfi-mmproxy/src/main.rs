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

use std::sync::Arc;

use darkfi::{async_daemonize, cli_desc, rpc::util::JsonValue, Error, Result};
use log::{debug, error, info};
use serde::Deserialize;
use smol::{net::TcpStream, stream::StreamExt, Executor};
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use surf::StatusCode;
use url::Url;

const CONFIG_FILE: &str = "darkfi_mmproxy.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../darkfi_mmproxy.toml");

/// Monero RPC functions
mod monerod;

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "darkfi-mmproxy", about = cli_desc!())]
struct Args {
    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(long)]
    /// Set log file output
    log: Option<String>,

    #[structopt(flatten)]
    mmproxy: MmproxyArgs,

    #[structopt(flatten)]
    monerod: MonerodArgs,
}

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[structopt()]
struct MmproxyArgs {
    #[structopt(long, default_value = "http://127.0.0.1:3333")]
    /// darkfi-mmproxy JSON-RPC server listen URL
    rpc: Url,
}

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[structopt()]
struct MonerodArgs {
    #[structopt(long, default_value = "mainnet")]
    /// Monero network type (mainnet/testnet)
    network: String,

    #[structopt(long, default_value = "http://127.0.0.1:18081")]
    /// monerod JSON-RPC server listen URL
    rpc: Url,
}

/// Mining proxy state
struct MiningProxy {
    /// monerod network type
    monerod_network: monero::Network,
    /// monerod RPC address
    monerod_rpc: Url,
}

impl MiningProxy {
    /// Instantiate `MiningProxy` state
    async fn new(monerod: MonerodArgs) -> Result<Self> {
        let monerod_network = match monerod.network.to_lowercase().as_str() {
            "mainnet" => monero::Network::Mainnet,
            "testnet" => monero::Network::Testnet,
            _ => {
                error!("Invalid Monero network \"{}\"", monerod.network);
                return Err(Error::Custom(format!("Invalid Monero network \"{}\"", monerod.network)))
            }
        };

        // Test that monerod RPC is reachable
        if let Err(e) = TcpStream::connect(monerod.rpc.socket_addrs(|| None)?[0]).await {
            error!("Failed connecting to monerod RPC: {}", e);
            return Err(e.into())
        }

        Ok(Self { monerod_network, monerod_rpc: monerod.rpc })
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'static>>) -> Result<()> {
    info!("Starting DarkFi x Monero merge mining proxy");

    let mmproxy = Arc::new(MiningProxy::new(args.monerod).await?);
    let mut app = tide::with_state(mmproxy);

    // monerod `/getheight` endpoint proxy
    app.at("/getheight").get(|req: tide::Request<Arc<MiningProxy>>| async move {
        let mmproxy = req.state();
        let return_data = mmproxy.monerod_get_height().await?;
        let return_data = return_data.stringify()?;
        debug!(target: "monerod::getheight", "<-- {}", return_data);
        Ok(return_data)
    });

    // monerod `/getinfo` endpoint proxy
    app.at("/getinfo").get(|req: tide::Request<Arc<MiningProxy>>| async move {
        let mmproxy = req.state();
        let return_data = mmproxy.monerod_get_info().await?;
        let return_data = return_data.stringify()?;
        debug!(target: "monerod::getinfo", "<-- {}", return_data);
        Ok(return_data)
    });

    // monerod `/json_rpc` endpoint proxy
    app.at("/json_rpc").post(|mut req: tide::Request<Arc<MiningProxy>>| async move {
        let json_str: JsonValue = match req.body_string().await {
            Ok(v) => v.parse()?,
            Err(e) => return Err(e),
        };

        let JsonValue::Object(ref request) = json_str else {
            return Err(surf::Error::new(
                StatusCode::BadRequest,
                Error::Custom("Invalid JSONRPC request".to_string()),
            ))
        };

        if !request.contains_key("method") || !request["method"].is_string() {
            return Err(surf::Error::new(
                StatusCode::BadRequest,
                Error::Custom("Invalid JSONRPC request".to_string()),
            ))
        }

        let mmproxy = req.state();
        let method = request["method"].get::<String>().unwrap();

        // For XMRig we only have to handle 2 methods:
        let return_data = match method.as_str() {
            "getblocktemplate" => mmproxy.monerod_getblocktemplate(&json_str).await?,
            "submitblock" => mmproxy.monerod_submit_block(&json_str).await?,
            _ => {
                return Err(surf::Error::new(
                    StatusCode::BadRequest,
                    Error::Custom("Invalid JSONRPC request".to_string()),
                ))
            }
        };

        let return_data = return_data.stringify()?;
        let log_tgt = format!("monerod::{}", method);
        debug!(target: &log_tgt,  "<-- {}", return_data);
        Ok(return_data)
    });

    ex.spawn(async move { app.listen(args.mmproxy.rpc).await.unwrap() }).detach();
    info!("Merge mining proxy ready, waiting for connections");

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting");

    Ok(())
}
