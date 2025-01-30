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

use std::{collections::HashMap, sync::Arc};

use darkfi::{
    async_daemonize, cli_desc,
    rpc::{
        jsonrpc::{JsonRequest, JsonResponse},
        util::JsonValue,
    },
    Error, Result,
};
use log::{debug, error, info};
use serde::Deserialize;
use smol::{stream::StreamExt, Executor};
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use surf::StatusCode;
use url::Url;

const CONFIG_FILE: &str = "darkfi_mmproxy.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../darkfi_mmproxy.toml");

/// Monero RPC functions
mod monerod;
use monerod::MonerodRequest;

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
    mmproxy_rpc: Url,
}

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[structopt()]
struct MonerodArgs {
    #[structopt(long, default_value = "mainnet")]
    /// Monero network type (mainnet/testnet)
    monero_network: String,

    #[structopt(long, default_value = "http://127.0.0.1:18081")]
    /// monerod JSON-RPC server listen URL
    monero_rpc: Url,
}

/// Mining proxy state
struct MiningProxy {
    /// Monero network type
    monero_network: monero::Network,
    /// Monero RPC address
    monero_rpc: Url,
}

impl MiningProxy {
    /// Instantiate `MiningProxy` state
    async fn new(monerod: MonerodArgs) -> Result<Self> {
        let monero_network = match monerod.monero_network.to_lowercase().as_str() {
            "mainnet" => monero::Network::Mainnet,
            "testnet" => monero::Network::Testnet,
            _ => {
                error!("Invalid Monero network \"{}\"", monerod.monero_network);
                return Err(Error::Custom(format!(
                    "Invalid Monero network \"{}\"",
                    monerod.monero_network
                )))
            }
        };

        // Test that monerod RPC is reachable and is configured
        // with the matching network
        let self_ = Self { monero_network, monero_rpc: monerod.monero_rpc };

        let req = JsonRequest::new("getinfo", vec![].into());
        let rep: JsonResponse = match self_.monero_request(MonerodRequest::Post(req)).await {
            Ok(v) => JsonResponse::try_from(&v)?,
            Err(e) => {
                error!("Failed connecting to monerod RPC: {}", e);
                return Err(e)
            }
        };

        let Some(result) = rep.result.get::<HashMap<String, JsonValue>>() else {
            error!("Invalid response from monerod RPC");
            return Err(Error::Custom("Invalid response from monerod RPC".to_string()))
        };

        let nettype = result.get("nettype").unwrap().get::<String>().unwrap();

        let mut xmr_is_mainnet = false;
        let mut xmr_is_testnet = false;

        match nettype.as_str() {
            // Here we allow fakechain, which we get with monerod --regtest
            "mainnet" | "fakechain" => xmr_is_mainnet = true,
            "testnet" => xmr_is_testnet = true,
            _ => unimplemented!("Missing handler for network {}", nettype),
        }

        if xmr_is_mainnet && monero_network != monero::Network::Mainnet {
            error!("mmproxy requested testnet, but monerod is mainnet");
            return Err(Error::Custom("Monero network mismatch".to_string()))
        }

        if xmr_is_testnet && monero_network != monero::Network::Testnet {
            error!("mmproxy requested mainnet, but monerod is testnet");
            return Err(Error::Custom("Monero network mismatch".to_string()))
        }

        Ok(self_)
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'static>>) -> Result<()> {
    info!("Starting DarkFi x Monero merge mining proxy");

    let mmproxy = Arc::new(MiningProxy::new(args.monerod).await?);
    let mut app = tide::with_state(mmproxy);

    // monerod `/getheight` endpoint proxy [HTTP GET]
    app.at("/getheight").get(|req: tide::Request<Arc<MiningProxy>>| async move {
        debug!(target: "monerod::getheight", "--> /getheight");
        let mmproxy = req.state();
        let return_data = mmproxy.monerod_get_height().await?;
        let return_data = return_data.stringify()?;
        debug!(target: "monerod::getheight", "<-- {}", return_data);
        Ok(return_data)
    });

    // monerod `/getinfo` endpoint proxy [HTTP GET]
    app.at("/getinfo").get(|req: tide::Request<Arc<MiningProxy>>| async move {
        debug!(target: "monerod::getinfo", "--> /getinfo");
        let mmproxy = req.state();
        let return_data = mmproxy.monerod_get_info().await?;
        let return_data = return_data.stringify()?;
        debug!(target: "monerod::getinfo", "<-- {}", return_data);
        Ok(return_data)
    });

    // monerod `/json_rpc` endpoint proxy [HTTP POST]
    app.at("/json_rpc").post(|mut req: tide::Request<Arc<MiningProxy>>| async move {
        let body_string = match req.body_string().await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "monerod::json_rpc", "Failed reading request body: {}", e);
                return Err(surf::Error::new(StatusCode::BadRequest, Error::Custom(e.to_string())))
            }
        };
        debug!(target: "monerod::json_rpc", "--> {}", body_string);

        let json_str: JsonValue = match body_string.parse() {
            Ok(v) => v,
            Err(e) => {
                error!(target: "monerod::json_rpc", "Failed parsing JSON body: {}", e);
                return Err(surf::Error::new(StatusCode::BadRequest, Error::Custom(e.to_string())))
            }
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

        // For XMRig we only have to handle 2 methods:
        let return_data: JsonValue = match request["method"].get::<String>().unwrap().as_str() {
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
        debug!(target: "monerod::json_rpc",  "<-- {}", return_data);
        Ok(return_data)
    });

    ex.spawn(async move { app.listen(args.mmproxy.mmproxy_rpc).await.unwrap() }).detach();
    info!("Merge mining proxy ready, waiting for connections");

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting");

    Ok(())
}
