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

use std::{fmt, sync::Arc};

use smol::stream::StreamExt;
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use tracing::info;
use url::Url;

use darkfi::{
    async_daemonize,
    blockchain::BlockInfo,
    cli_desc,
    rpc::{client::RpcClient, jsonrpc::JsonRequest, util::JsonValue},
    util::encoding::base64,
    Result,
};
use darkfi_serial::{deserialize, serialize};

const CONFIG_FILE: &str = "blockchain_storage_metrics_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../blockchain_storage_metrics_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "blockchain-storage-metrics", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(short, long, default_value = "tcp://127.0.0.1:8340")]
    /// darkfid JSON-RPC endpoint
    endpoint: Url,

    #[structopt(short, long)]
    /// Block height to measure until
    height: Option<usize>,

    #[structopt(short, long)]
    /// Set log file to output into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

/// Structure representing block storage metrics.
/// Everything is measured in bytes.
struct BlockMetrics {
    /// Header height
    height: u32,
    /// Header hash,
    hash: String,
    /// Header size
    header_size: usize,
    /// Transactions count
    txs: usize,
    /// Transactions size
    txs_size: usize,
    /// Block producer signature size
    signature_size: usize,
}

impl BlockMetrics {
    fn new(block: &BlockInfo) -> Self {
        let header_size = serialize(&block.header).len();
        let txs_size = serialize(&block.txs).len();
        let signature_size = serialize(&block.signature).len();
        Self {
            height: block.header.height,
            hash: block.hash().to_string(),
            header_size,
            txs: block.txs.len(),
            txs_size,
            signature_size,
        }
    }

    /// Compute Block total raw size, in bytes.
    fn size(&self) -> usize {
        self.header_size + self.txs_size + self.signature_size
    }
}

impl fmt::Display for BlockMetrics {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = format!(
            "Block {} - {} metrics:\n\t{}: {}\n\t{}: {}\n\t{}: {}\n\t{}: {}\n\t{}: {}",
            self.height,
            self.hash,
            "Header size",
            self.header_size,
            "Transactions count",
            self.txs,
            "Transactions size",
            self.txs_size,
            "Block producer signature size",
            self.signature_size,
            "Raw size",
            self.size(),
        );

        write!(f, "{}", s)
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    // DEV-NOTE: We store everything in memory so don't go crazy with it
    info!(target: "blockchain-storage-metrics", "Initializing blockchain storage metrics script...");

    // Initialize rpc client
    let rpc_client = RpcClient::new(args.endpoint, ex).await?;

    // Grab all blocks up to configured height
    let height = args.height.unwrap_or_default() + 1;
    let mut blocks = Vec::with_capacity(height);
    for h in 0..height {
        info!(target: "blockchain-storage-metrics", "Requesting block for height: {h}");
        let req = JsonRequest::new(
            "blockchain.get_block",
            JsonValue::Array(vec![JsonValue::Number(h as f64)]),
        );
        let rep = rpc_client.request(req).await?;
        let encoded_block = rep.get::<String>().unwrap();
        let bytes = base64::decode(encoded_block).unwrap();
        let block: BlockInfo = deserialize(&bytes)?;
        info!(target: "blockchain-storage-metrics", "Retrieved block: {h} - {}", block.hash());
        blocks.push(block);
    }

    // Stop rpc client
    rpc_client.stop().await;

    // TODO: Create a dummy in memory validator to apply each block

    // Measure each block storage
    let mut blocks_metrics = Vec::with_capacity(height);
    for block in &blocks {
        // TODO: Grab complete storage requirements from the validator
        let block_metrics = BlockMetrics::new(block);
        info!(target: "blockchain-storage-metrics", "{block_metrics}");
        blocks_metrics.push(block_metrics);
    }

    // Measure total storage
    let mut total_headers_size = 0_u64;
    let mut total_txs = 0_u64;
    let mut total_txs_size = 0_u64;
    let mut total_signatures_size = 0_u64;
    let mut total_size = 0_u64;
    for block_metrics in blocks_metrics {
        total_headers_size += block_metrics.header_size as u64;
        total_txs += block_metrics.txs as u64;
        total_txs_size += block_metrics.txs_size as u64;
        total_signatures_size += block_metrics.signature_size as u64;
        total_size += block_metrics.size() as u64;
    }
    let metrics = format!(
        "Total metrics:\n\t{}: {}\n\t{}: {}\n\t{}: {}\n\t{}: {}\n\t{}: {}",
        "Headers size",
        total_headers_size,
        "Transactions",
        total_txs,
        "Transactions size",
        total_txs_size,
        "Signatures size",
        total_signatures_size,
        "Raw size",
        total_size
    );
    info!(target: "blockchain-storage-metrics", "{metrics}");

    // TODO: export metrics as a csv so we can use it to visualize stuff in charts

    Ok(())
}
